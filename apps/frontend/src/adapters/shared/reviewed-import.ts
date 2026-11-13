import { invoke } from "./platform";

export interface ReviewedImportAccount {
  id: string;
  name: string;
  currency: "INR" | "USD";
}
export interface ReviewedImportStatus {
  supported: boolean;
  accounts: ReviewedImportAccount[];
}
export interface ReviewedPrice {
  amount: string;
  currency: string;
  observedOn: string;
  source: string;
  venue: string;
}
export interface ReviewedEquity {
  isin: string;
  symbol: string;
  name: string;
  currency: string;
  sourceVenue: string;
  valuationVenue: string;
  ordinaryEquityConfirmed: boolean;
  quantity: string;
  averageCost: string | null;
  price: ReviewedPrice | null;
}
export interface ReviewedSnapshotInput {
  schemaVersion: 1;
  accountId: string;
  currency: string;
  source: { label: string; account: string };
  asOf: string;
  settledScope: "complete" | "partial";
  otherExposure: "none_confirmed" | "unknown" | "unresolved";
  cash: Record<string, string> | null;
  positions: ReviewedEquity[];
}
export interface ImportReceipt {
  schemaVersion: number;
  reviewId: string;
  inputHash: string;
  accountId: string;
  snapshotId: string;
  asOf: string;
  importedAt: string;
}
export interface ImportReview {
  reviewId: string;
  inputHash: string;
  expiresAt: string;
  accountName: string;
  snapshot: ReviewedSnapshotInput;
  changes: {
    isin: string;
    kind:
      | "added"
      | "changed"
      | "removed_from_complete_snapshot"
      | "not_covered_by_partial_snapshot";
  }[];
  warnings: string[];
}
export interface SavedReviewedSnapshot {
  receipt: ImportReceipt;
  snapshot: ReviewedSnapshotInput;
  assetIds: Record<string, string>;
  historyAvailable: false;
}
export type PreparedReviewedImport =
  | { status: "review"; review: ImportReview }
  | { status: "already_imported"; receipt: ImportReceipt };

export const getReviewedImportStatus = (): Promise<ReviewedImportStatus> =>
  invoke("get_reviewed_import_status");
export const createReviewedImportAccount = (request: {
  id: string;
  name: string;
  currency: "INR" | "USD";
}): Promise<ReviewedImportAccount> => invoke("create_reviewed_import_account", { request });
export const prepareReviewedImport = (
  request: ReviewedSnapshotInput,
): Promise<PreparedReviewedImport> => invoke("prepare_reviewed_import", { request });
export const confirmReviewedImport = (request: {
  reviewId: string;
  inputHash: string;
}): Promise<ImportReceipt> => invoke("confirm_reviewed_import", { request });
export const discardReviewedImport = (request: { reviewId: string }): Promise<void> =>
  invoke("discard_reviewed_import", { request });
export const latestReviewedImport = (request: {
  accountId: string;
}): Promise<SavedReviewedSnapshot | null> => invoke("latest_reviewed_import", { request });
