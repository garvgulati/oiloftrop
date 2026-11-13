import type { ReviewedImportAccount } from "@/adapters";
import { PrivacyProvider } from "@/context/privacy-context";
import { Toaster } from "@/components/sonner";
import { useProfile } from "@/features/profiles/profile-context";
import ResearchPage from "@/features/research/research-page";
import { StockResearch } from "@/features/research/stock-research";
import { SettingsProvider } from "@/lib/settings-provider";
import { Button, TooltipProvider } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { BrowserRouter, Navigate, NavLink, Route, Routes } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { PersonalHoldingsHome } from "./holdings-home";

/** Personal routes deliberately omit Connect, addons, generic AI and analytics providers. */
export function PersonalApplication({ accounts }: { accounts: ReviewedImportAccount[] }) {
  const { t } = useTranslation("common");
  const profile = useProfile();
  const queryClient = useQueryClient();
  const id = profile?.profile?.id ?? "none";
  return (
    <SettingsProvider>
      <PrivacyProvider>
        <TooltipProvider>
          <Toaster closeButton />
          <BrowserRouter>
            <div className="bg-background text-foreground mx-auto min-h-screen max-w-5xl space-y-5 p-4 pb-24 sm:p-6">
              <header className="space-y-3 border-b pb-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <strong className="text-lg">Oiloftrop · {t("personal.private")}</strong>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      onClick={() =>
                        void queryClient.invalidateQueries({
                          queryKey: ["reviewed-import-status", id],
                        })
                      }
                    >
                      {t("refresh")}
                    </Button>
                    <Button variant="outline" onClick={() => profile?.manageProfile()}>
                      {t("personal.profile")}
                    </Button>
                    <Button variant="outline" onClick={() => profile?.lockProfile()}>
                      {t("personal.lock")}
                    </Button>
                  </div>
                </div>
                <nav className="flex flex-wrap gap-2" aria-label={t("personal.navigation")}>
                  {[
                    ["/", "personal.home"],
                    ["/india", "researchIndia.india"],
                    ["/us", "researchIndia.us"],
                    ["/metals", "personal.metals"],
                    ["/research", "personal.manageHoldings"],
                  ].map(([path, key]) => (
                    <NavLink
                      key={path}
                      to={path}
                      end={path === "/"}
                      className={({ isActive }) =>
                        `min-h-11 rounded-md border px-3 py-2 text-sm ${isActive ? "bg-accent font-medium" : "bg-background"}`
                      }
                    >
                      {t(key)}
                    </NavLink>
                  ))}
                </nav>
              </header>
              <Routes>
                <Route path="/" element={<PersonalHoldingsHome key={id} accounts={accounts} />} />
                <Route
                  path="/india"
                  element={<StockResearch key={`${id}-india`} profileId={id} fixedMarket="india" />}
                />
                <Route
                  path="/us"
                  element={<StockResearch key={`${id}-us`} profileId={id} fixedMarket="us" />}
                />
                <Route path="/research" element={<ResearchPage />} />
                <Route
                  path="/metals"
                  element={
                    <section className="space-y-3">
                      <h1 className="text-2xl font-semibold">{t("personal.metals")}</h1>
                      <p>{t("personal.metalsPending")}</p>
                    </section>
                  }
                />
                <Route path="*" element={<Navigate to="/" replace />} />
              </Routes>
            </div>
          </BrowserRouter>
        </TooltipProvider>
      </PrivacyProvider>
    </SettingsProvider>
  );
}
