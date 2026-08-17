import { useEffect, useState } from "react";

import { loadAppConfig } from "../../app/config";
import { apiClient } from "../../shared/api/client";

import "./DiscordConnect.css";

const DISCORD_HOSTS = new Set(["discord.gg", "discord.com"]);
const DISCORD_INVITE_TOKEN = /^[A-Za-z0-9_-]+$/;

type DiscordInviteResponse = Readonly<{
  invite_url: string | null;
}>;

/**
 * Accept only an HTTPS Discord invite. Credentials, arbitrary hosts, and
 * non-invite Discord paths are rejected before reaching the browser.
 */
export function safeDiscordInviteUrl(value: string | undefined): string | undefined {
  if (!value?.trim()) return undefined;

  let url: URL;
  try {
    url = new URL(value.trim());
  } catch {
    return undefined;
  }

  if (url.protocol !== "https:" || url.username || url.password || url.port) return undefined;
  const host = url.hostname.toLowerCase();
  if (!DISCORD_HOSTS.has(host)) return undefined;
  const token = host === "discord.com"
    ? url.pathname.replace(/^\/invite\//i, "")
    : url.pathname.slice(1);
  if (!url.pathname.startsWith(host === "discord.com" ? "/invite/" : "/")
      || !DISCORD_INVITE_TOKEN.test(token)) return undefined;

  return url.toString();
}

export type DiscordConnectProps = Readonly<{
  inviteUrl?: string;
}>;

export function DiscordConnect({ inviteUrl }: DiscordConnectProps = {}) {
  const buildTimeInviteUrl = loadAppConfig().discordInviteUrl;
  const [runtimeInviteUrl, setRuntimeInviteUrl] = useState<string | undefined>();
  const [runtimeConfigLoaded, setRuntimeConfigLoaded] = useState(
    Boolean(inviteUrl ?? buildTimeInviteUrl),
  );

  useEffect(() => {
    if (inviteUrl !== undefined || buildTimeInviteUrl !== undefined) return undefined;

    let mounted = true;
    void apiClient
      .get<DiscordInviteResponse>("/discord/invite")
      .then((response) => {
        if (!mounted) return;
        setRuntimeInviteUrl(response.invite_url ?? undefined);
        setRuntimeConfigLoaded(true);
      })
      .catch(() => {
        if (mounted) setRuntimeConfigLoaded(true);
      });

    return () => {
      mounted = false;
    };
  }, [buildTimeInviteUrl, inviteUrl]);

  const configuredInviteUrl = inviteUrl ?? buildTimeInviteUrl ?? runtimeInviteUrl;
  const safeInviteUrl = safeDiscordInviteUrl(configuredInviteUrl);

  return (
    <main aria-labelledby="discord-connect-title" className="discord-connect">
      <h1 id="discord-connect-title">Join the ORION community</h1>
      <p>Ask questions, share learning progress, and connect with other learners.</p>
      {!runtimeConfigLoaded ? (
        <p role="status" aria-live="polite">Loading Discord community link…</p>
      ) : safeInviteUrl ? (
        <a
          href={safeInviteUrl}
          referrerPolicy="no-referrer"
          rel="noopener noreferrer"
          target="_blank"
        >
          Join ORION on Discord
        </a>
      ) : (
        <p role="status">The Discord community link is not available right now.</p>
      )}
    </main>
  );
}

// The approved invite is supplied by the server-side configuration registry
// (with a build-time local-development fallback) and is validated again before
// it reaches an anchor element.
