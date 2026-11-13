import {
  prepareReviewedImport,
  confirmReviewedImport,
  discardReviewedImport,
  latestReviewedImport,
  createReviewedImportAccount,
  type ReviewedImportAccount,
  type ImportReview,
  type ReviewedSnapshotInput,
  type SavedReviewedSnapshot,
} from "@/adapters";
import { Button, Input } from "@wealthfolio/ui";
import { useEffect, useRef, useState } from "react";
import { useFieldArray, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import {
  blankEquity,
  blankReviewedDraft,
  draftInput,
  type ReviewedDraft,
} from "./reviewed-import-draft";

const inputClass = "bg-background min-h-11 w-full rounded-md border p-2 text-sm";

export function ReviewedImportPanel({
  accounts: suppliedAccounts,
  onAccountCreated,
}: {
  accounts: ReviewedImportAccount[];
  onAccountCreated?: () => void;
}) {
  const { t } = useTranslation("common");
  const [createdAccounts, setCreatedAccounts] = useState<ReviewedImportAccount[]>([]);
  const accounts = [
    ...suppliedAccounts,
    ...createdAccounts.filter((a) => !suppliedAccounts.some((s) => s.id === a.id)),
  ];
  const [accountName, setAccountName] = useState("");
  const [accountCurrency, setAccountCurrency] = useState<"INR" | "USD">("INR");
  const accountRequest = useRef<{ id: string; name: string; currency: "INR" | "USD" } | null>(null);
  const { register, control, handleSubmit, watch, setValue } = useForm<ReviewedDraft>({
    defaultValues: blankReviewedDraft(),
  });
  const rows = useFieldArray({ control, name: "positions" });
  const accountId = watch("accountId");
  const account = accounts.find((a) => a.id === accountId);
  const venues =
    account?.currency === "USD" ? ["XNAS", "XNYS", "XASE", "ARCX", "BATS"] : ["XNSE", "XBOM"];
  const [review, setReview] = useState<ImportReview | null>(null);
  const [saved, setSaved] = useState<SavedReviewedSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<
    | "invalid"
    | "failed"
    | "expired"
    | "savedReloadFailed"
    | "accountFailed"
    | "priceAfterRecord"
    | null
  >(null);
  const [savedConfirmed, setSavedConfirmed] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const lifecycle = useRef({ active: true, revision: 0, read: 0 });
  const pending = useRef<ImportReview | null>(null);
  const previewElement = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (review) previewElement.current?.focus();
  }, [review]);

  useEffect(() => {
    const owner = lifecycle.current;
    owner.active = true;
    return () => {
      owner.active = false;
      owner.revision++;
      owner.read++;
      const value = pending.current;
      if (value) void discardReviewedImport({ reviewId: value.reviewId }).catch(() => undefined);
    };
  }, []);
  useEffect(() => {
    if (!review) return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [review]);
  useEffect(() => {
    setSaved(null);
    setSavedConfirmed(false);
    lifecycle.current.read++;
  }, [accountId]);

  const clearReview = () => {
    const previous = pending.current;
    pending.current = null;
    setReview(null);
    if (previous)
      void discardReviewedImport({ reviewId: previous.reviewId }).catch(() => undefined);
  };
  const edit = () => {
    lifecycle.current.revision++;
    clearReview();
    setError(null);
    setSavedConfirmed(false);
  };
  const load = async (id: string, afterSave = false) => {
    const request = ++lifecycle.current.read;
    try {
      const result = await latestReviewedImport({ accountId: id });
      if (lifecycle.current.active && request === lifecycle.current.read) setSaved(result);
    } catch {
      if (lifecycle.current.active && request === lifecycle.current.read)
        setError(afterSave ? "savedReloadFailed" : "failed");
    }
  };
  const prepare = handleSubmit(async (raw) => {
    clearReview();
    setError(null);
    setSavedConfirmed(false);
    let input: ReviewedSnapshotInput;
    try {
      input = draftInput(raw, accounts);
    } catch (validation) {
      setError(
        validation instanceof z.ZodError &&
          validation.issues.some((issue) => issue.message === "PRICE_AFTER_RECORD")
          ? "priceAfterRecord"
          : "invalid",
      );
      return;
    }
    const version = lifecycle.current.revision;
    setBusy(true);
    try {
      const result = await prepareReviewedImport(input);
      if (!lifecycle.current.active || version !== lifecycle.current.revision) {
        if (result.status === "review")
          void discardReviewedImport({ reviewId: result.review.reviewId }).catch(() => undefined);
        return;
      }
      if (result.status === "already_imported") {
        setSavedConfirmed(true);
        await load(input.accountId, true);
      } else {
        pending.current = result.review;
        setReview(result.review);
        setClock(Date.now());
      }
    } catch {
      if (lifecycle.current.active && version === lifecycle.current.revision) setError("failed");
    } finally {
      if (lifecycle.current.active) setBusy(false);
    }
  });
  const confirm = async () => {
    if (!review) return;
    if (Date.parse(review.expiresAt) <= Date.now()) {
      setError("expired");
      clearReview();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await confirmReviewedImport({ reviewId: review.reviewId, inputHash: review.inputHash });
      if (!lifecycle.current.active) return;
      pending.current = null;
      setReview(null);
      setSavedConfirmed(true);
      await load(review.snapshot.accountId, true);
    } catch {
      if (lifecycle.current.active) setError("failed");
    } finally {
      if (lifecycle.current.active) setBusy(false);
    }
  };
  const label = (key: string) => t(`reviewedImport.${key}`);
  const createAccount = async () => {
    const name = accountName.trim();
    if (!name) {
      setError("accountFailed");
      return;
    }
    edit();
    setBusy(true);
    if (
      accountRequest.current?.name !== name ||
      accountRequest.current?.currency !== accountCurrency
    ) {
      accountRequest.current = { id: crypto.randomUUID(), name, currency: accountCurrency };
    }
    try {
      const created = await createReviewedImportAccount(accountRequest.current);
      if (!lifecycle.current.active) return;
      setCreatedAccounts((previous) => [...previous.filter((a) => a.id !== created.id), created]);
      setValue("accountId", created.id);
      setAccountName("");
      accountRequest.current = null;
      onAccountCreated?.();
    } catch {
      if (lifecycle.current.active) setError("accountFailed");
    } finally {
      if (lifecycle.current.active) setBusy(false);
    }
  };
  return (
    <section className="space-y-4 rounded-xl border p-4" aria-labelledby="reviewed-import-title">
      <h2 id="reviewed-import-title" className="text-lg font-medium">
        {label("title")}
      </h2>
      <p className="text-muted-foreground text-sm">{label("scope")}</p>
      <p className="text-sm">{label("saveSteps")}</p>
      <form
        autoComplete="off"
        onSubmit={(event) => {
          event.preventDefault();
          void createAccount();
        }}
      >
        <fieldset disabled={busy} className="space-y-3 rounded-lg border p-3">
          <legend className="px-1 font-medium">{label("newAccount")}</legend>
          <p className="text-muted-foreground text-sm">{label("newAccountNote")}</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="space-y-1 text-sm">
              <span>{label("newAccountName")}</span>
              <Input
                value={accountName}
                maxLength={160}
                onChange={(event) => setAccountName(event.target.value)}
              />
            </label>
            <label className="space-y-1 text-sm">
              <span>{label("newAccountCurrency")}</span>
              <select
                className={inputClass}
                value={accountCurrency}
                onChange={(event) => setAccountCurrency(event.target.value as "INR" | "USD")}
              >
                <option value="INR">INR</option>
                <option value="USD">USD</option>
              </select>
            </label>
          </div>
          <Button type="submit" disabled={!accountName.trim()}>
            {label("createAccount")}
          </Button>
        </fieldset>
      </form>
      {busy && <p role="status">{t("loading")}</p>}
      {error === "accountFailed" && <p role="alert">{label(error)}</p>}
      {accounts.length === 0 ? (
        <p role="status">{label("accountSetupRequired")}</p>
      ) : (
        <>
          <form
            onSubmit={(event) => void prepare(event)}
            onChange={edit}
            className="space-y-4"
            autoComplete="off"
          >
            <fieldset disabled={busy} className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="space-y-1 text-sm">
                  <span>{label("targetAccount")}</span>
                  <select {...register("accountId")} className={inputClass}>
                    <option value="">{label("choose")}</option>
                    {accounts.map((a) => (
                      <option value={a.id} key={a.id}>
                        {a.name} · {a.currency}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="space-y-1 text-sm">
                  <span>{label("sourceDate")}</span>
                  <Input
                    type="date"
                    aria-label={label("sourceDate")}
                    aria-describedby="reviewed-source-date-help"
                    {...register("asOf")}
                  />
                  <span
                    id="reviewed-source-date-help"
                    className="text-muted-foreground block text-xs"
                  >
                    {label("sourceDateHelp")}
                  </span>
                </label>
                <label className="space-y-1 text-sm">
                  <span>{label("sourceLabel")}</span>
                  <Input maxLength={160} {...register("label")} />
                </label>
                <label className="space-y-1 text-sm">
                  <span>{label("sourceAccount")}</span>
                  <Input maxLength={160} {...register("sourceAccount")} />
                </label>
                <label className="space-y-1 text-sm">
                  <span>{label("settledScope")}</span>
                  <select className={inputClass} {...register("settledScope")}>
                    <option value="partial">{label("partial")}</option>
                    <option value="complete">{label("complete")}</option>
                  </select>
                </label>
                <label className="space-y-1 text-sm">
                  <span>{label("otherExposure")}</span>
                  <select className={inputClass} {...register("otherExposure")}>
                    <option value="unknown">{label("unknown")}</option>
                    <option value="unresolved">{label("unresolved")}</option>
                    <option value="none_confirmed">{label("noneConfirmed")}</option>
                  </select>
                </label>
                <label className="space-y-1 text-sm">
                  <span>
                    {label("cash")} {account?.currency}
                  </span>
                  <Input inputMode="decimal" placeholder={label("unknown")} {...register("cash")} />
                </label>
              </div>
              <p className="text-muted-foreground text-sm">{label("unknownNote")}</p>
              {rows.fields.map((row, index) => (
                <fieldset key={row.id} className="space-y-3 rounded-lg border p-3">
                  <legend className="px-1 font-medium">
                    {label("stock")} {index + 1}
                  </legend>
                  <div className="grid gap-3 sm:grid-cols-2">
                    {(
                      [
                        "isin",
                        "symbol",
                        "name",
                        "quantity",
                        "averageCost",
                        "price",
                        "priceDate",
                        "priceSource",
                      ] as const
                    ).map((field) => (
                      <label key={field} className="space-y-1 text-sm">
                        <span>{label(field)}</span>
                        <Input
                          type={field === "priceDate" ? "date" : "text"}
                          aria-label={field === "priceDate" ? label(field) : undefined}
                          aria-describedby={
                            field === "priceDate" ? `reviewed-price-date-help-${index}` : undefined
                          }
                          inputMode={
                            ["quantity", "averageCost", "price"].includes(field)
                              ? "decimal"
                              : "text"
                          }
                          {...register(`positions.${index}.${field}`)}
                        />
                        {field === "priceDate" && (
                          <span
                            id={`reviewed-price-date-help-${index}`}
                            className="text-muted-foreground block text-xs"
                          >
                            {label("priceDateHelp")}
                          </span>
                        )}
                      </label>
                    ))}
                    {(["sourceVenue", "valuationVenue"] as const).map((field) => (
                      <label key={field} className="space-y-1 text-sm">
                        <span>{label(field)}</span>
                        <select className={inputClass} {...register(`positions.${index}.${field}`)}>
                          <option value="">{label("choose")}</option>
                          {venues.map((mic) => (
                            <option key={mic} value={mic}>
                              {mic}
                            </option>
                          ))}
                        </select>
                      </label>
                    ))}
                  </div>
                  <label className="flex min-h-11 items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      {...register(`positions.${index}.ordinaryEquityConfirmed`)}
                    />
                    <span>{label("ordinaryEquity")}</span>
                  </label>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => {
                      edit();
                      rows.remove(index);
                    }}
                  >
                    {label("removeStock")}
                  </Button>
                </fieldset>
              ))}
              {error && error !== "accountFailed" && !review && <p role="alert">{label(error)}</p>}
              <div className="flex flex-wrap gap-3">
                <Button
                  type="button"
                  variant="outline"
                  disabled={rows.fields.length >= 200}
                  onClick={() => {
                    edit();
                    rows.append(blankEquity());
                  }}
                >
                  {label("addStock")}
                </Button>
                <Button type="submit">{label("prepare")}</Button>
                <Button
                  type="button"
                  variant="outline"
                  disabled={!accountId}
                  onClick={() => void load(accountId)}
                >
                  {label("reload")}
                </Button>
              </div>
            </fieldset>
          </form>
          {review && (
            <div
              ref={previewElement}
              tabIndex={-1}
              className="space-y-3 rounded-lg border p-3"
              aria-label={label("preview")}
            >
              <h3 className="font-medium">
                {label("preview")} · {review.accountName}
              </h3>
              <ReviewedSnapshotDetails snapshot={review.snapshot} />
              <p className="text-sm">
                {label("expires")}: {new Date(review.expiresAt).toLocaleString()}
              </p>
              <ul className="list-disc space-y-1 pl-5 text-sm">
                {review.warnings.map((warning) => (
                  <li key={warning}>{label(`warnings.${warning}`)}</li>
                ))}
              </ul>
              <ul className="list-disc space-y-1 pl-5 text-sm">
                {review.changes.map((change) => (
                  <li key={change.isin}>
                    {change.isin}: {label(`changes.${change.kind}`)}
                  </li>
                ))}
              </ul>
              {error && error !== "accountFailed" && <p role="alert">{label(error)}</p>}
              <div className="flex flex-wrap gap-3">
                <Button
                  disabled={busy || Date.parse(review.expiresAt) <= clock}
                  onClick={() => void confirm()}
                >
                  {label("confirm")}
                </Button>
                <Button disabled={busy} variant="outline" onClick={clearReview}>
                  {t("cancel")}
                </Button>
              </div>
            </div>
          )}
          {savedConfirmed && <p role="status">{label("saved")}</p>}
          {saved && (
            <div className="space-y-3 rounded-lg border p-3" aria-label={label("savedRecord")}>
              <h3 className="font-medium">{label("savedRecord")}</h3>
              <ReviewedSnapshotDetails snapshot={saved.snapshot} />
              <p className="text-muted-foreground text-sm">{label("historyUnavailable")}</p>
            </div>
          )}
        </>
      )}
    </section>
  );
}

export function ReviewedSnapshotDetails({ snapshot }: { snapshot: ReviewedSnapshotInput }) {
  const { t } = useTranslation("common");
  const label = (key: string) => t(`reviewedImport.${key}`);
  const unknown = label("unknown");
  return (
    <div className="space-y-3 break-words text-sm">
      <p>
        {label("sourceLabel")}: {snapshot.source.label} · {snapshot.source.account}
      </p>
      <p>
        {label("sourceDate")}: {snapshot.asOf} · {snapshot.currency}
      </p>
      <p>
        {label("settledScope")}: {label(snapshot.settledScope)} · {label("otherExposure")}:{" "}
        {label(
          snapshot.otherExposure === "none_confirmed" ? "noneConfirmed" : snapshot.otherExposure,
        )}
      </p>
      <p>
        {label("cash")}: {snapshot.cash?.[snapshot.currency] ?? unknown} {snapshot.currency}
      </p>
      {snapshot.positions.map((p) => (
        <div key={p.isin} className="space-y-1 border-t pt-2">
          <p className="font-medium">
            {p.name} · {p.symbol} · {p.isin}
          </p>
          <p>
            {label("quantity")}: {p.quantity} · {label("averageCost")}: {p.averageCost ?? unknown}{" "}
            {p.currency}
          </p>
          <p>
            {label("sourceVenue")}: {p.sourceVenue} · {label("valuationVenue")}: {p.valuationVenue}
          </p>
          <p>
            {label("price")}:{" "}
            {p.price
              ? `${p.price.amount} ${p.price.currency} · ${p.price.observedOn} · ${p.price.source} · ${p.price.venue}`
              : unknown}
          </p>
        </div>
      ))}
    </div>
  );
}
