import { invoke } from "./platform";
import type { PlanningProfile } from "../../../../../../wealth-manager/lib/assessment-contracts";

export interface PublicCompanyData {
  submissions: unknown;
  facts: unknown;
  retrievedAt: string;
}

export interface IndiaConnection {
  configured: boolean;
  expiresOn: string | null;
}
export interface IndiaConnectionInput {
  token: string;
  expiresOn: string;
  freePersonalConfirmed: boolean;
  privateDashboardConfirmed: boolean;
}
export interface IndiaResearchListing {
  name: string;
  symbol: string;
  isin: string;
  key: string;
  exchange: "NSE" | "BSE";
}
export interface IndiaResearchData {
  listing: IndiaResearchListing;
  basis: "consolidated" | "standalone";
  income: unknown;
  cash: unknown;
  balance: unknown;
  retrievedAt: string;
}
export const getIndiaResearchConnection = (): Promise<IndiaConnection> =>
  invoke<IndiaConnection>("get_india_research_connection");
export const setIndiaResearchConnection = (
  request: IndiaConnectionInput,
): Promise<IndiaConnection> =>
  invoke<IndiaConnection>("set_india_research_connection", { request });
export const removeIndiaResearchConnection = (): Promise<IndiaConnection> =>
  invoke<IndiaConnection>("remove_india_research_connection");
export const searchIndiaResearch = (
  query: string,
): Promise<{ listings: IndiaResearchListing[]; retrievedAt: string }> =>
  invoke("search_india_research", { request: { query } });
export const analyseIndiaResearch = (
  instrumentKey: string,
  basis: "consolidated" | "standalone",
): Promise<IndiaResearchData> =>
  invoke("analyse_india_research", { request: { instrumentKey, basis } });

export const loadResearchCompany = (symbol: string): Promise<PublicCompanyData> =>
  invoke<PublicCompanyData>("load_research_company", { request: { symbol } });

export interface IndiaChartData {
  listing: IndiaResearchListing;
  range: "day" | "month" | "year";
  interval: string;
  candles: {
    time: string;
    open: number;
    high: number;
    low: number;
    close: number;
    volume: number;
  }[];
  quote: { price: number; lastTradeAt: string };
  retrievedAt: string;
}
export interface IndiaRatiosData {
  listing: IndiaResearchListing;
  ratios: {
    name: "P/E" | "P/B" | "ROA" | "ROE" | "ROCE" | "EV/EBITDA";
    company: number | null;
    sector: number | null;
  }[];
  retrievedAt: string;
}
export const loadIndiaResearchRatios = (instrumentKey: string): Promise<IndiaRatiosData> =>
  invoke("load_india_research_ratios", { request: { instrumentKey } });
export const loadIndiaResearchChart = (
  instrumentKey: string,
  range: IndiaChartData["range"],
): Promise<IndiaChartData> =>
  invoke("load_india_research_chart", { request: { instrumentKey, range } });

export interface ResearchPlan {
  profile: PlanningProfile;
  cashLinks: { cashId: string; accountId: string }[];
}

export interface StoredResearchPlan {
  schemaVersion: 1;
  mode: "fictional" | "personal";
  revision: number;
  plan: ResearchPlan;
  createdAt: string;
  updatedAt: string;
}

export const getResearchPlan = (): Promise<StoredResearchPlan | null> =>
  invoke<StoredResearchPlan | null>("get_research_plan");

export const saveResearchPlan = (
  expectedRevision: number,
  plan: ResearchPlan,
): Promise<StoredResearchPlan> =>
  invoke<StoredResearchPlan>("save_research_plan", { request: { expectedRevision, plan } });

export interface ResearchQuote {
  price: string | null;
  adjustedPrice: string | null;
  currency: string;
  observationDate: string;
  observedAt: string;
  source: string;
  priceBasis: string;
}

export interface ResearchContext {
  schemaVersion: 1;
  capturedAt: string;
  valuationDate: string;
  baseCurrency: string;
  contextHash: string;
  planRevision: number;
  mode: "fictional" | "personal";
  plan: StoredResearchPlan | null;
  accounts: {
    id: string;
    name: string;
    currency: string;
    archived: boolean;
    snapshotDate: string | null;
    snapshotSource: string | null;
    cash: Record<string, string | null>;
  }[];
  excludedAccountIds: string[];
  positions: {
    accountId: string;
    assetId: string;
    quantity: string | null;
    averageCost: string | null;
    currency: string | null;
    contractMultiplier: string | null;
    instrumentType: string | null;
    symbol: string | null;
    exchangeMic: string | null;
    issuerId: string | null;
    shareClass: string | null;
    sector: string | null;
    quote: ResearchQuote | null;
  }[];
  fxQuotes: { assetId: string; pair: string; quote: ResearchQuote | null }[];
  issues: {
    code: string;
    accountId: string | null;
    assetId: string | null;
    field: string | null;
  }[];
  coverageComplete: boolean;
}

export const getResearchContext = (): Promise<ResearchContext> =>
  invoke<ResearchContext>("get_research_context");

export type ResearchExampleCase = "baseline" | "unknown-loss" | "changed-fee";

export interface ResearchExampleAssessment {
  protocolVersion: 1;
  engineBuildId: string;
  methodologyVersion: string;
  mode: "fictional";
  fixtureId: string;
  caseId: ResearchExampleCase;
  asOf: string;
  currency: "USD";
  inputHash: string;
  quantity: string;
  invested: string;
  entryCosts: string;
  cashRemainder: string;
  cards: { years: number[]; score: null; state: string }[];
  horizons: {
    years: number;
    end: string;
    remainingA: number | null;
    remainingB: number | null;
    difference: number | null;
    target: number | null;
    goalMetA: boolean | null;
    goalMetB: boolean | null;
  }[];
  reasonCodes: string[];
}

export const runResearchExample = (
  caseId: ResearchExampleCase,
): Promise<ResearchExampleAssessment> =>
  invoke<ResearchExampleAssessment>("run_research_example", { request: { caseId } });
