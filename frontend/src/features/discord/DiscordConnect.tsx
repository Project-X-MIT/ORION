import { loadAppConfig } from "../../app/config";

import "./DiscordConnect.css";

const DISCORD_HOSTS = new Set(["discord.gg", "discord.com"]);

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

  if (url.protocol !== "https:" || url.username || url.password) return undefined;
  const host = url.hostname.toLowerCase();
  if (!DISCORD_HOSTS.has(host)) return undefined;
  const approvedPath = host === "discord.com"
    ? /^\/invite\/[^/]+$/i
    : /^\/[^/]+$/;
  if (!approvedPath.test(url.pathname)) return undefined;

  return url.toString();
}

export type DiscordConnectProps = Readonly<{
  inviteUrl?: string;
}>;

export function DiscordConnect({ inviteUrl }: DiscordConnectProps = {}) {
  const configuredInviteUrl = inviteUrl ?? loadAppConfig().discordInviteUrl;
  const safeInviteUrl = safeDiscordInviteUrl(configuredInviteUrl);

  return (
    <section aria-labelledby="discord-connect-title" className="discord-connect">
      <h2 id="discord-connect-title">Join the ORION community</h2>
      <p>Ask questions, share learning progress, and connect with other learners.</p>
      {safeInviteUrl ? (
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
    </section>
  );
}

// TODO(Div): expose the approved runtime configuration field through the
// shared configuration registry when the frontend registry is registered.
