import { z } from "zod";
import type {
  ReviewedImportAccount,
  ReviewedSnapshotInput,
} from "@/adapters/shared/reviewed-import";

const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const label = z.string().trim().min(1).max(160);
const decimal = (positive = false) =>
  z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,8})?$/)
    .refine((value) => !positive || !/^0+(?:\.0+)?$/.test(value));
const optionalAmount = z.union([z.literal(""), decimal()]);
const equity = z
  .object({
    isin: z
      .string()
      .trim()
      .toUpperCase()
      .regex(/^[A-Z]{2}[A-Z0-9]{9}\d$/),
    symbol: label,
    name: label,
    sourceVenue: label,
    valuationVenue: label,
    ordinaryEquityConfirmed: z.boolean().refine(Boolean),
    quantity: decimal(true),
    averageCost: optionalAmount,
    price: z.string().trim(),
    priceDate: z.string(),
    priceSource: z.string().trim(),
  })
  .superRefine((row, context) => {
    const hasPrice = !!(row.price || row.priceDate || row.priceSource);
    if (
      hasPrice &&
      (!decimal(true).safeParse(row.price).success ||
        !date.safeParse(row.priceDate).success ||
        !label.safeParse(row.priceSource).success)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["price"],
        message: "PRICE_DETAILS_REQUIRED",
      });
    }
  });
export const reviewedDraftSchema = z
  .object({
    accountId: z.string().uuid(),
    label,
    sourceAccount: label,
    asOf: date,
    settledScope: z.enum(["partial", "complete"]),
    otherExposure: z.enum(["unknown", "unresolved", "none_confirmed"]),
    cash: optionalAmount,
    positions: z.array(equity).max(200),
  })
  .superRefine((draft, context) => {
    if (!date.safeParse(draft.asOf).success) return;
    draft.positions.forEach((row, index) => {
      if (date.safeParse(row.priceDate).success && row.priceDate > draft.asOf) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["positions", index, "priceDate"],
          message: "PRICE_AFTER_RECORD",
        });
      }
    });
  });
export type ReviewedDraft = z.infer<typeof reviewedDraftSchema>;
export const blankEquity = (): ReviewedDraft["positions"][number] => ({
  isin: "",
  symbol: "",
  name: "",
  sourceVenue: "",
  valuationVenue: "",
  ordinaryEquityConfirmed: false,
  quantity: "",
  averageCost: "",
  price: "",
  priceDate: "",
  priceSource: "",
});
export const blankReviewedDraft = (): ReviewedDraft => ({
  accountId: "",
  label: "",
  sourceAccount: "",
  asOf: "",
  settledScope: "partial",
  otherExposure: "unknown",
  cash: "",
  positions: [blankEquity()],
});
export function draftInput(
  raw: ReviewedDraft,
  accounts: ReviewedImportAccount[],
): ReviewedSnapshotInput {
  const draft = reviewedDraftSchema.parse(raw);
  const account = accounts.find((a) => a.id === draft.accountId);
  if (!account) throw new Error("PERSONAL_IMPORT_ACCOUNT_REQUIRED");
  const venues =
    account.currency === "INR" ? ["XNSE", "XBOM"] : ["XNAS", "XNYS", "XASE", "ARCX", "BATS"];
  if (
    draft.positions.some(
      (p) => !venues.includes(p.sourceVenue) || !venues.includes(p.valuationVenue),
    )
  ) {
    throw new Error("PERSONAL_IMPORT_VENUE_INVALID");
  }
  return {
    schemaVersion: 1,
    accountId: draft.accountId,
    currency: account.currency,
    source: { label: draft.label, account: draft.sourceAccount },
    asOf: draft.asOf,
    settledScope: draft.settledScope,
    otherExposure: draft.otherExposure,
    cash: draft.cash === "" ? null : { [account.currency]: draft.cash },
    positions: draft.positions.map((row) => ({
      isin: row.isin,
      symbol: row.symbol,
      name: row.name,
      currency: account.currency,
      sourceVenue: row.sourceVenue,
      valuationVenue: row.valuationVenue,
      ordinaryEquityConfirmed: row.ordinaryEquityConfirmed,
      quantity: row.quantity,
      averageCost: row.averageCost === "" ? null : row.averageCost,
      price:
        row.price === ""
          ? null
          : {
              amount: row.price,
              currency: account.currency,
              observedOn: row.priceDate,
              source: row.priceSource,
              venue: row.valuationVenue,
            },
    })),
  };
}
