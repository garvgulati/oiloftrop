import { getReviewedImportStatus, type ReviewedImportStatus } from "@/adapters";
import { StartupScreen } from "@/components/startup-screen";
import { useProfile } from "@/features/profiles/profile-context";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@wealthfolio/ui";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { reloadApplication } from "@/lib/reload-application";
import { useTranslation } from "react-i18next";

interface RuntimeGateProps {
  children: ReactNode;
  personal: (status: ReviewedImportStatus) => ReactNode;
}

/** Resolve host mode before mounting providers with portfolio/egress behavior. */
export function RuntimeGate({ children, personal }: RuntimeGateProps) {
  const profileId = useProfile()?.profile?.id;
  const { t } = useTranslation("common");
  const [admitted, setAdmitted] = useState<boolean | undefined>();
  const status = useQuery({
    queryKey: ["reviewed-import-status", profileId],
    queryFn: getReviewedImportStatus,
    enabled: !!profileId,
    retry: false,
    staleTime: 0,
    gcTime: 0,
  });
  const supported = status.data?.supported;
  const valid =
    typeof supported === "boolean" &&
    Array.isArray(status.data?.accounts) &&
    status.data.accounts.every(
      (account) =>
        account &&
        typeof account === "object" &&
        typeof account.id === "string" &&
        typeof account.name === "string" &&
        (account.currency === "INR" || account.currency === "USD"),
    );
  useEffect(() => {
    if (admitted === undefined && valid) setAdmitted(supported);
  }, [admitted, supported, valid]);
  const changed =
    admitted !== undefined &&
    typeof status.data?.supported === "boolean" &&
    admitted !== status.data.supported;
  if (changed)
    return (
      <main className="mx-auto max-w-lg space-y-4 p-6" role="alert">
        <p>{t("personal.modeChanged")}</p>
        <Button onClick={() => reloadApplication()}>{t("personal.reloadApp")}</Button>
      </main>
    );
  if (status.isPending || (admitted === undefined && valid)) return <StartupScreen />;
  if (valid && admitted === true && status.data?.supported === true) return personal(status.data);
  if (valid && admitted === false && status.data?.supported === false) return children;
  return (
    <main className="mx-auto max-w-lg space-y-4 p-6" role="alert">
      <p>{t("personal.modeError")}</p>
      <Button onClick={() => void status.refetch()}>{t("refresh")}</Button>
    </main>
  );
}
