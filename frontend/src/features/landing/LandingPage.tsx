import "./LandingPage.css";
import { useEffect, useRef } from "react";

const leaderboardNames = [
  "Mira K.", "Alex R.", "You", "Sam T.", "Nila P.", "Rohan V.", "Leah M.", "Iris J.", "Noah C.", "Avery B.",
  "Owen T.", "Maya S.", "Liam D.", "Zara P.", "Theo H.", "Aria N.", "Eli W.", "June F.", "Kai L.", "Sana R.",
  "Mateo G.", "Nora V.", "Ezra K.", "Asha M.", "Milo B.", "Lina C.", "Jonah S.", "Vera D.", "Amir P.", "Cleo H.",
  "Rhea N.", "Ishan W.", "Luca F.", "Elena L.", "Soren R.", "Anya G.", "Kian V.", "Mina K.", "Omar J.", "Ari C.",
  "Dev S.", "Tara P.", "Ivan M.", "Riya B.", "Nico D.", "Sara H.", "Quinn T.", "Yuna E.", "Cora M.", "Jai P.",
  "Aiden R.", "Bella N.", "Caleb V.", "Dina K.", "Emil T.", "Faye M.", "Gavin J.", "Hana C.", "Idris B.", "Jules S.",
  "Kara D.", "Leo P.", "Mira V.", "Niko H.", "Olia W.", "Pavel F.", "Quinn L.", "Rina G.", "Sami R.", "Talia N.",
  "Uma K.", "Vik M.", "Wren B.", "Xena C.", "Yara S.", "Zane D.", "Ayla P.", "Bram H.", "Cami J.", "Drew L.",
  "Enzo M.", "Fia R.", "Gio T.", "Hugo V.", "Inez K.", "Juno N.", "Kobe S.", "Lara W.", "Miko F.", "Naya G.",
  "Mia T.", "Niko R.", "Orla V.", "Pia K.", "Remy H.", "Sia B.", "Toby C.", "Usha D.", "Wade F.", "Xavi L.",
] as const;

const leaderboardScores = ["9,842", "9,410", "8,920", "8,640", "8,210"] as const;
const leaderboard = leaderboardNames.map((name, index) => [name, leaderboardScores[index] ?? (8210 - (index - 4) * 117).toLocaleString("en-US")] as const);
const leaderboardLoop = [...leaderboard, ...leaderboard.slice(0, 6)];

const news = [
  { tag: "MARKETS", title: "The quiet shift behind this week’s volatility", time: "6 min read", variant: "markets" },
  { tag: "SCIENCE", title: "Why better questions create better forecasts", time: "4 min read", variant: "science" },
  { tag: "WORLD", title: "Three signals worth carrying into tomorrow", time: "8 min read", variant: "world" },
] as const;

const signalCandles = [
  { x: 20, high: 105, low: 136, open: 128, close: 118, className: "is-up" },
  { x: 54, high: 88, low: 121, open: 96, close: 110, className: "is-down" },
  { x: 88, high: 86, low: 126, open: 118, close: 101, className: "is-up" },
  { x: 122, high: 58, low: 104, open: 71, close: 91, className: "is-up" },
  { x: 156, high: 61, low: 107, open: 74, close: 92, className: "is-down" },
  { x: 190, high: 42, low: 94, open: 82, close: 57, className: "is-up" },
  { x: 224, high: 48, low: 88, open: 61, close: 76, className: "is-down" },
  { x: 258, high: 30, low: 80, open: 69, close: 43, className: "is-up" },
  { x: 292, high: 18, low: 70, open: 31, close: 53, className: "is-down" },
  { x: 326, high: 28, low: 68, open: 58, close: 36, className: "is-up" },
  { x: 360, high: 13, low: 55, open: 25, close: 44, className: "is-down" },
  { x: 394, high: 8, low: 47, open: 39, close: 16, className: "is-up" },
  { x: 428, high: 5, low: 35, open: 12, close: 27, className: "is-down" },
] as const;

const highTrendPath = signalCandles.map(({ x, high }, index) => `${index === 0 ? "M" : "L"} ${x} ${high}`).join(" ");
const lowTrendPath = signalCandles.map(({ x, low }, index) => `${index === 0 ? "M" : "L"} ${x} ${low}`).join(" ");

function SignalChart() {
  return <div className="landing-chart-wrap">
    <div className="landing-chart-legend"><span><i /> Trading signals</span><strong>+24.8%</strong></div>
    <svg aria-hidden="true" className="landing-signal-chart" viewBox="0 0 440 150" preserveAspectRatio="none">
      <g className="landing-signal-chart__grid">
        <path d="M0 24H440M0 60H440M0 96H440M0 132H440" />
        <path d="M55 0V150M110 0V150M165 0V150M220 0V150M275 0V150M330 0V150M385 0V150" />
      </g>
      <path className="landing-signal-chart__trendline landing-signal-chart__trendline--high" d={highTrendPath} />
      <path className="landing-signal-chart__trendline landing-signal-chart__trendline--low" d={lowTrendPath} />
      <g className="landing-signal-chart__candles">
        {signalCandles.map(({ x, high, low, open, close, className }) => <g key={x} className={className}>
          <line x1={x} x2={x} y1={high} y2={low} />
          <rect className={className} x={x - 5} y={Math.min(open, close)} width="10" height={Math.max(3, Math.abs(open - close))} />
        </g>)}
      </g>
    </svg>
    <div className="landing-chart-axis"><span>09:00</span><span>12:00</span><span>15:00</span><span>18:00</span></div>
  </div>;
}

function NewsThumbnail({ variant }: { variant: "markets" | "science" | "world" }) {
  return <svg aria-hidden="true" className={`landing-news-thumb landing-news-thumb--${variant}`} viewBox="0 0 160 96" preserveAspectRatio="none">
          <rect width="160" height="96" rx="12" fill="#070914" />
          <path d="M12 72H148M12 48H148M12 24H148" stroke="#8eb7ff" opacity=".2" />
    {variant === "markets" && <><path d="M12 69 C28 61 32 66 44 51 S65 56 76 42 S98 48 110 29 S131 39 148 18" fill="none" stroke="currentColor" strokeWidth="3" /><circle cx="110" cy="29" r="4" fill="currentColor" /><rect x="28" y="58" width="5" height="12" rx="2" fill="currentColor" opacity=".7" /><rect x="63" y="48" width="5" height="14" rx="2" fill="currentColor" opacity=".7" /><rect x="99" y="36" width="5" height="13" rx="2" fill="currentColor" opacity=".7" /></>}
    {variant === "science" && <><path d="M28 66L63 34L94 57L132 24" fill="none" stroke="currentColor" strokeWidth="2" opacity=".8" /><circle cx="28" cy="66" r="7" fill="currentColor" /><circle cx="63" cy="34" r="6" fill="currentColor" opacity=".8" /><circle cx="94" cy="57" r="9" fill="currentColor" opacity=".65" /><circle cx="132" cy="24" r="7" fill="currentColor" /></>}
    {variant === "world" && <><circle cx="80" cy="48" r="28" fill="none" stroke="currentColor" strokeWidth="2" /><path d="M52 48H108M80 20C68 30 68 66 80 76M80 20C92 30 92 66 80 76M58 32C70 38 90 38 102 32M58 64C70 58 90 58 102 64" fill="none" stroke="currentColor" strokeWidth="2" opacity=".8" /><circle cx="123" cy="25" r="4" fill="currentColor" /></>}
  </svg>;
}

function LandingBrand() {
  return <a className="landing-brand" href="/" aria-label="ORION home">
    <span>ORION</span>
  </a>;
}

export function LandingPage() {
  const pageRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const page = pageRef.current;
    if (!page || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    page.classList.add("landing-page--motion-ready");
    const revealTargets = page.querySelectorAll<HTMLElement>("[data-reveal]");
    let frame = 0;
    const updateScrollMotion = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        page.style.setProperty("--landing-scroll-shift", `${-Math.min(90, window.scrollY * 0.08)}px`);
        frame = 0;
      });
    };
    const cleanupScrollMotion = () => {
      window.removeEventListener("scroll", updateScrollMotion);
      if (frame) window.cancelAnimationFrame(frame);
    };

    window.addEventListener("scroll", updateScrollMotion, { passive: true });
    updateScrollMotion();

    if (!("IntersectionObserver" in window)) {
      revealTargets.forEach((target) => target.classList.add("is-visible"));
      return cleanupScrollMotion;
    }

    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("is-visible");
        observer.unobserve(entry.target);
      });
    }, { threshold: 0.16 });

    revealTargets.forEach((target) => observer.observe(target));
    return () => {
      observer.disconnect();
      cleanupScrollMotion();
    };
  }, []);

  return <div ref={pageRef} className="landing-page">
    <a className="landing-skip-link" href="#landing-main">Skip to main content</a>

    <header className="landing-header">
      <LandingBrand />
      <nav aria-label="Landing page navigation" className="landing-nav">
        <a href="/discord">Discord</a>
        <a href="/quiz">Practice Quiz</a>
        <a href="#leaderboard">Leaderboard</a>
        <a href="#reports">Reports</a>
        <a href="#news">News</a>
      </nav>
      <div className="landing-header__actions">
        <a href="/login">Sign in</a>
        <a className="landing-header__cta" href="/login">Start learning</a>
      </div>
    </header>

    <main id="landing-main">
      <section className="landing-hero" aria-labelledby="landing-title">
        <div className="landing-hero__copy">
          <p className="landing-hero__brandline"><strong>ORION</strong><span>INTELLIGENCE NETWORK</span></p>
          <h1 id="landing-title">Learn for the <em>world.</em></h1>
          <p className="landing-hero__intro">Build the signal, challenge your assumptions, and understand what moves the world before it becomes obvious.</p>
          <div className="landing-hero__actions">
            <a className="landing-button landing-button--dark" href="/login">Start learning <span aria-hidden="true">↗</span></a>
          </div>
          <div className="landing-hero__stats" aria-label="ORION community statistics">
            <span><strong>120k+</strong> signals explored</span>
            <span><strong>4.8k</strong> active thinkers</span>
          </div>
        </div>

        <div className="landing-hero__stage" aria-label="ORION trading notebook preview">
          <article className="landing-notebook">
            <div className="landing-notebook__cover">
              <header><span>ORION / RESEARCH PAPER</span><strong>024</strong></header>
              <div className="landing-notebook__title"><span>TRADING SIGNALS</span><h2>Read the move.<br /><em>Know the why.</em></h2></div>
              <SignalChart />
              <div className="landing-notebook__knowledge"><span>KNOWLEDGE / 01</span><strong>Signal is not noise.</strong><p>Read the chart, then follow the idea behind it.</p></div>
              <footer><span>PRIVATE PREVIEW</span><span>ORION DESK ↗</span></footer>
            </div>
            <span className="landing-notebook__page landing-notebook__page--one" aria-hidden="true" />
            <span className="landing-notebook__page landing-notebook__page--two" aria-hidden="true" />
          </article>
        </div>
      </section>

      <section id="features" className="landing-section landing-features" data-reveal aria-labelledby="features-title">
        <div className="landing-section__heading">
          <p className="landing-eyebrow">THE ORION SYSTEM</p>
          <h2 id="features-title">Get more out of<br /><em>every signal.</em></h2>
          <p>Browse public research papers before you sign in and read the full report.</p>
        </div>
        <div id="reports" className="landing-feature-grid landing-feature-grid--reports">
          <article className="landing-feature-card landing-feature-card--reports">
            <span className="landing-feature-card__number">NETWORK REPORTS / PUBLIC PREVIEWS</span>
            <h3>See how other<br />people think.</h3>
    <div className="landing-report-list"><a href="/login"><span className="landing-report-folder" aria-hidden="true"><i /><b /><em /></span><span className="landing-report-meta">MARKETS / 06 MIN</span><strong>What happens when attention becomes scarce?</strong><small>Preview: The first signal appears in how people choose what to ignore.</small><b>Sign in to read more ↗</b></a><a href="/login"><span className="landing-report-folder" aria-hidden="true"><i /><b /><em /></span><span className="landing-report-meta">SCIENCE / 04 MIN</span><strong>Can better questions make better forecasts?</strong><small>Preview: A short research paper on curiosity, evidence, and conviction.</small><b>Sign in to read more ↗</b></a></div>
          </article>
        </div>
      </section>

      <section id="leaderboard" className="landing-section landing-leaderboard-section" data-reveal aria-labelledby="leaderboard-title">
        <article className="landing-leaderboard-panel">
          <div className="landing-dashboard-card__heading"><h2 id="leaderboard-title">Leaderboard</h2></div>
          <div className="landing-leaderboard-layout">
            <div className="landing-leaderboard-list" role="list" aria-label="Leaderboard standings">{leaderboardLoop.map(([name, score], index) => { const rank = index % leaderboard.length; return <div className={`landing-leaderboard-row${name === "You" ? " is-you" : ""}`} key={`${name}-${index}`} role="listitem"><span className="landing-leaderboard-rank">{String(rank + 1).padStart(2, "0")}</span><span className="landing-avatar">{name.slice(0, 1)}</span><strong>{name}</strong><span className="landing-leaderboard-score">{score}</span></div>; })}</div>
          </div>
        </article>
      </section>

      <section id="news" className="landing-section landing-news-section" data-reveal aria-labelledby="news-title">
        <div className="landing-news-layout">
          <article className="landing-dashboard-card landing-dashboard-card--news">
            <div className="landing-dashboard-card__heading"><div><p className="landing-eyebrow">THE SIGNAL FEED</p><h2 id="news-title">News</h2></div><a href="/news" aria-label="Open news">↗</a></div>
            <div className="landing-news-list">{news.map((story) => <a className="landing-news-row" href="/news" key={story.title}><NewsThumbnail variant={story.variant} /><span className="landing-news-row__tag">{story.tag}</span><strong>{story.title}</strong><span className="landing-news-row__time">{story.time} <b aria-hidden="true">↗</b></span></a>)}</div>
            <a className="landing-card-link" href="/news">Read all signals <span aria-hidden="true">↗</span></a>
          </article>
        </div>
      </section>

      <section className="landing-final" data-reveal aria-labelledby="landing-final-title">
        <div className="landing-final__orb" aria-hidden="true"><span /><i /><b /></div>
        <p className="landing-eyebrow">YOUR NEXT SIGNAL IS WAITING</p>
        <h2 id="landing-final-title">Think further.<br /><em>Move earlier.</em></h2>
        <a className="landing-button landing-button--lime" href="/login">Start learning <span aria-hidden="true">↗</span></a>
      </section>
    </main>

    <footer className="landing-footer"><LandingBrand /><span>Learn · Compete · Move beyond.</span><a href="/login">Sign in ↗</a></footer>
  </div>;
}
