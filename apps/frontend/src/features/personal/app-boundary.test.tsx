import type { ReactNode } from "react";
import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import App from "@/App";

const api = vi.hoisted(() => ({
  getReviewedImportStatus: vi.fn(),
  connect: vi.fn(),
  addons: vi.fn(),
  logos: vi.fn(),
  routes: vi.fn(),
}));
vi.mock("@/adapters", () => ({
  isWeb: true,
  getReviewedImportStatus: api.getReviewedImportStatus,
}));
vi.mock("@/features/profiles/profile-context", () => ({
  useProfile: () => ({ profile: { id: "synthetic-profile" } }),
}));
vi.mock("@/features/profiles/profile-shell", () => ({
  ProfileShell: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/features/database-recovery/native-database-gate", () => ({
  NativeDatabaseGate: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/context/auth-context", () => ({
  AuthProvider: ({ children }: { children: ReactNode }) => children,
  AuthGate: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/lib/settings-provider", () => ({
  SettingsProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/context/privacy-context", () => ({
  PrivacyProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/features/wealthfolio-connect", () => ({
  WealthfolioConnectProvider: ({ children }: { children: ReactNode }) => {
    api.connect();
    return children;
  },
}));
vi.mock("@/addons/addon-runtime-loader", () => ({
  AddonRuntimeLoader: () => {
    api.addons();
    return null;
  },
}));
vi.mock("@/addons/addons-runtime-context", () => ({ setAddonQueryClient: vi.fn() }));
vi.mock("@/components/asset-logo-registry-sync", () => ({
  AssetLogoRegistrySync: () => {
    api.logos();
    return null;
  },
}));
vi.mock("@/components/sonner", () => ({ Toaster: () => null }));
vi.mock("@/features/spending/components/event-dialog-provider", () => ({
  EventDialogProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/pages/auth/login-page", () => ({ LoginPage: () => <p>Synthetic sign in</p> }));
vi.mock("@/routes", () => ({
  AppRoutes: () => {
    api.routes();
    return <p>Synthetic normal app</p>;
  },
}));
vi.mock("./personal-application", () => ({
  PersonalApplication: () => <p>Synthetic personal app</p>,
}));
vi.mock("@/components/startup-screen", () => ({
  StartupScreen: () => <p>Synthetic pending mode</p>,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
beforeEach(() => vi.clearAllMocks());

it("the actual App never mounts Connect, addons, logo or generic route providers for a personal host", async () => {
  let resolve!: (value: unknown) => void;
  api.getReviewedImportStatus.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  render(<App />);
  await screen.findByText("Synthetic pending mode");
  for (const spy of [api.connect, api.addons, api.logos, api.routes])
    expect(spy).not.toHaveBeenCalled();
  await act(async () => {
    resolve({ supported: true, accounts: [] });
    await Promise.resolve();
  });
  await screen.findByText("Synthetic personal app");
  for (const spy of [api.connect, api.addons, api.logos, api.routes])
    expect(spy).not.toHaveBeenCalled();
});
it("the actual App preserves the existing provider tree for explicit normal/fictional mode", async () => {
  api.getReviewedImportStatus.mockResolvedValue({ supported: false, accounts: [] });
  render(<App />);
  await screen.findByText("Synthetic normal app");
  for (const spy of [api.connect, api.addons, api.logos, api.routes])
    expect(spy).toHaveBeenCalled();
});
