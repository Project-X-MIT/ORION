import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AppProviders, useAppConfig } from "./AppProviders";
import { useAuth } from "./AuthProvider";
import { useTheme } from "./ThemeProvider";

function FoundationConsumer() {
  const config = useAppConfig();
  const auth = useAuth();
  const theme = useTheme();

  return (
    <output
      data-api-base={config.apiBaseUrl}
      data-auth-status={auth.status}
      data-theme={theme.theme}
    >
      foundation-ready
    </output>
  );
}

describe("AppProviders", () => {
  it("composes config, theme, query, and authentication contexts", () => {
    const markup = renderToStaticMarkup(
      <AppProviders>
        <FoundationConsumer />
      </AppProviders>,
    );

    expect(markup).toContain("foundation-ready");
    expect(markup).toContain('data-api-base="/api/v1"');
    expect(markup).toContain('data-auth-status="loading"');
    expect(markup).toContain('data-theme="light"');
  });
});
