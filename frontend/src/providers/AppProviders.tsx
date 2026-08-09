import {
  Component,
  createContext,
  useContext,
  type ErrorInfo,
  type PropsWithChildren,
  type ReactNode,
} from "react";

import { loadAppConfig, type AppConfig } from "../app/config";
import { AuthProvider } from "./AuthProvider";
import { QueryProvider } from "./QueryProvider";
import { ThemeProvider } from "./ThemeProvider";

const AppConfigContext = createContext<AppConfig | undefined>(undefined);

type BootstrapErrorBoundaryState = { error: Error | null };

class BootstrapErrorBoundary extends Component<PropsWithChildren, BootstrapErrorBoundaryState> {
  state: BootstrapErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): BootstrapErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("ORION application bootstrap failed", error, info);
  }

  retry = () => this.setState({ error: null });

  render(): ReactNode {
    if (this.state.error) {
      return (
        <main role="alert">
          <h1>ORION could not start</h1>
          <p>{this.state.error.message}</p>
          <button type="button" onClick={this.retry}>Try again</button>
        </main>
      );
    }

    return this.props.children;
  }
}

function BootstrapProviders({ children }: PropsWithChildren) {
  const config = loadAppConfig();

  return (
    <AppConfigContext.Provider value={config}>
      <ThemeProvider>
        <QueryProvider>
          <AuthProvider>{children}</AuthProvider>
        </QueryProvider>
      </ThemeProvider>
    </AppConfigContext.Provider>
  );
}

export function AppProviders({ children }: PropsWithChildren) {
  return (
    <BootstrapErrorBoundary>
      <BootstrapProviders>{children}</BootstrapProviders>
    </BootstrapErrorBoundary>
  );
}

export function useAppConfig(): AppConfig {
  const config = useContext(AppConfigContext);
  if (!config) throw new Error("useAppConfig must be used inside AppProviders");
  return config;
}
