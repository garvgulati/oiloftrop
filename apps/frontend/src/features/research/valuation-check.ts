export interface ValuationInputs {
  price: string;
  priceDate: string;
  priceSource: string;
  eps: string;
  periodStart: string;
  periodEnd: string;
  filedDate: string;
  earningsSource: string;
  multiple: string;
  rationale: string;
  confirmed: boolean;
}

export const emptyValuation: ValuationInputs = {
  price: "",
  priceDate: "",
  priceSource: "",
  eps: "",
  periodStart: "",
  periodEnd: "",
  filedDate: "",
  earningsSource: "",
  multiple: "",
  rationale: "",
  confirmed: false,
};

const decimal = (text: string) => (/^-?\d+(\.\d{1,8})?$/.test(text) ? Number(text) : NaN);
const day = (text: string) => {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(text)) return NaN;
  const value = Date.parse(`${text}T00:00:00Z`);
  return Number.isFinite(value) && new Date(value).toISOString().slice(0, 10) === text
    ? value
    : NaN;
};
const source = (text: string) => {
  try {
    const url = new URL(text);
    return url.protocol === "https:" && !url.username && !url.password;
  } catch {
    return false;
  }
};

/** Indicative, user-reviewed arithmetic. Not an automated valuation or trade instruction. */
export function checkValuation(input: ValuationInputs, today: string) {
  const price = decimal(input.price),
    eps = decimal(input.eps);
  const priceDay = day(input.priceDate),
    start = day(input.periodStart),
    end = day(input.periodEnd);
  const filed = day(input.filedDate),
    now = day(today);
  if (!input.confirmed) return { error: "confirm" } as const;
  if (![price, eps].every(Number.isFinite) || price <= 0 || price > 1e9 || Math.abs(eps) > 1e9)
    return { error: "numbers" } as const;
  if (
    ![priceDay, start, end, filed, now].every(Number.isFinite) ||
    start > end ||
    end > filed ||
    filed >= priceDay ||
    priceDay > now ||
    (end - start) / 86400000 < 330 ||
    (end - start) / 86400000 > 380
  )
    return { error: "dates" } as const;
  if (!source(input.priceSource) || !source(input.earningsSource))
    return { error: "sources" } as const;
  if (eps <= 0) return { error: "loss" } as const;
  const multiple = input.multiple === "" ? null : decimal(input.multiple);
  if (
    multiple !== null &&
    (!Number.isFinite(multiple) || multiple <= 0 || multiple > 1000 || !input.rationale.trim())
  )
    return { error: "multipleError" } as const;
  const impliedPrice = multiple === null ? null : eps * multiple;
  return {
    error: null,
    pe: price / eps,
    yield: (eps / price) * 100,
    impliedPrice,
    difference: impliedPrice === null ? null : (impliedPrice / price - 1) * 100,
    priceAge: Math.floor((now - priceDay) / 86400000),
    earningsAge: Math.floor((priceDay - end) / 86400000),
  } as const;
}
