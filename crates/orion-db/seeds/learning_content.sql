-- Deterministic IDs make this seed safe to reference from lesson rows and
-- make local development data reproducible across database resets.
INSERT INTO course_modules (
    id, slug, title, description, display_order, is_published
)
VALUES
    (
        '10000000-0000-0000-0000-000000000001',
        'market-foundations',
        'Market Foundations',
        'Learn what markets are, how prices move, and the vocabulary used by traders.',
        1,
        TRUE
    ),
    (
        '10000000-0000-0000-0000-000000000002',
        'reading-the-market',
        'Reading the Market',
        'Build a simple framework for reading candles, trends, and important price levels.',
        2,
        TRUE
    ),
    (
        '10000000-0000-0000-0000-000000000003',
        'risk-and-trade-planning',
        'Risk and Trade Planning',
        'Plan trades around a defined risk amount instead of emotion or prediction.',
        3,
        TRUE
    ),
    (
        '10000000-0000-0000-0000-000000000004',
        'building-a-routine',
        'Building a Trading Routine',
        'Turn the ideas from this course into a repeatable research, execution, and review process.',
        4,
        TRUE
    )
ON CONFLICT (id) DO UPDATE SET
    slug = EXCLUDED.slug,
    title = EXCLUDED.title,
    description = EXCLUDED.description,
    display_order = EXCLUDED.display_order,
    is_published = EXCLUDED.is_published,
    updated_at = CURRENT_TIMESTAMP;

INSERT INTO course_lessons (
    id, module_id, slug, title, summary, content, lesson_order,
    estimated_minutes, is_published
)
VALUES
    (
        '20000000-0000-0000-0000-000000000001',
        '10000000-0000-0000-0000-000000000001',
        'what-is-a-market',
        'What Is a Market?',
        'Understand markets as agreements between buyers and sellers.',
        $$A market is a place where buyers and sellers exchange an asset. The quoted price is not a promise that an asset is worth that amount forever; it is the price at which a trade can happen now.

Prices change when the balance between willing buyers and sellers changes. Your first job as a beginner is to describe that behavior clearly before trying to predict it.$$,
        1,
        8,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000002',
        '10000000-0000-0000-0000-000000000001',
        'stocks-etfs-and-indexes',
        'Stocks, ETFs, and Indexes',
        'Learn how common market instruments differ and what ownership means.',
        $$A stock represents a share of a company. An exchange-traded fund (ETF) holds a basket of assets and trades throughout the day like a stock. An index is a measurement of a group of assets; you usually gain exposure to it through a fund or another derivative.

Before trading, identify exactly what you are buying, how it is priced, and what risks can make its price move.$$,
        2,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000003',
        '10000000-0000-0000-0000-000000000001',
        'orders-and-spreads',
        'Orders and Spreads',
        'See how market, limit, and stop orders behave in a live market.',
        $$A market order prioritizes execution, while a limit order prioritizes price. A stop order becomes active after a trigger price is reached. The bid is the highest visible buying price and the ask is the lowest visible selling price; the difference is the spread.

Execution is part of risk. A fast-moving or illiquid market can fill you at a less favorable price than expected, so understand the order type before you submit it.$$,
        3,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000004',
        '10000000-0000-0000-0000-000000000002',
        'reading-candles',
        'Reading Candles',
        'Use open, high, low, and close to describe what happened during a period.',
        $$A candle records four prices for a time period: open, high, low, and close. The body shows the distance between open and close. The wicks show the highest and lowest prices reached.

One candle is rarely a complete signal. Read candles in context: compare them with nearby candles, the broader trend, volume when available, and the price levels that matter to your plan.$$,
        1,
        12,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000005',
        '10000000-0000-0000-0000-000000000002',
        'trends-and-ranges',
        'Trends and Ranges',
        'Distinguish directional movement from a market that is moving sideways.',
        $$An uptrend generally forms higher highs and higher lows. A downtrend forms lower highs and lower lows. In a range, price repeatedly moves between an upper and lower area without establishing a sustained direction.

The same setup can mean different things in a trend and in a range. Start by naming the environment; only then decide whether a strategy is appropriate.$$,
        2,
        12,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000006',
        '10000000-0000-0000-0000-000000000002',
        'support-and-resistance',
        'Support and Resistance',
        'Mark areas where previous buying or selling changed the path of price.',
        $$Support and resistance are zones, not perfectly precise lines. They form where market participants previously showed enough interest to slow or reverse price.

Treat a level as a location for a decision, not as a guarantee. Define what would confirm your idea and what would invalidate it before the market reaches the zone.$$,
        3,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000007',
        '10000000-0000-0000-0000-000000000003',
        'define-risk-first',
        'Define Risk First',
        'Choose the maximum acceptable loss before choosing a position size.',
        $$A trade plan starts with the amount you are willing to lose if the idea is wrong. That amount should be small enough that one loss does not damage your finances or decision-making.

Risk is not the same as the amount invested. A larger position with a close invalidation point can carry less risk than a smaller position with no exit plan. Always define the invalidation point first.$$,
        1,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000008',
        '10000000-0000-0000-0000-000000000003',
        'position-sizing',
        'Position Sizing',
        'Calculate a position from account risk and the distance to your stop.',
        $$A simple sizing model is: position size = money at risk divided by risk per unit. For example, risking $50 with a $2 planned loss per share gives a maximum of 25 shares before fees and slippage.

Use conservative assumptions. Fees, spreads, gaps, and partial fills can make the actual result worse than the paper calculation.$$,
        2,
        12,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000009',
        '10000000-0000-0000-0000-000000000003',
        'risk-reward-and-expectancy',
        'Risk, Reward, and Expectancy',
        'Evaluate a plan across many trades instead of judging one outcome.',
        $$A favorable risk-to-reward ratio does not guarantee a profitable strategy, and a high win rate does not remove risk. Expectancy combines average win, average loss, and the probabilities of each outcome.

Judge a method over a meaningful sample. A single winning trade can be lucky, while a single losing trade can be normal variance.$$,
        3,
        12,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000010',
        '10000000-0000-0000-0000-000000000004',
        'build-a-watchlist',
        'Build a Watchlist',
        'Focus limited attention on instruments that match your plan and liquidity needs.',
        $$A useful watchlist is small enough to review consistently. Record the symbol, the market environment, the levels that matter, and the event risks that could change the picture.

Do not add an instrument just because it moved dramatically. Volatility can create opportunity, but it can also increase slippage and the chance of a larger-than-planned loss.$$,
        1,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000011',
        '10000000-0000-0000-0000-000000000004',
        'keep-a-trading-journal',
        'Keep a Trading Journal',
        'Capture decisions and outcomes so your process can improve over time.',
        $$For every trade, record the setup, entry, invalidation point, planned risk, exit, and what you were thinking. Add a chart or screenshot when useful. The goal is to review decisions, not to create a perfect story after the fact.

Review the journal on a schedule. Look for repeated process errors such as moving stops, oversizing, or trading outside your plan.$$,
        2,
        10,
        TRUE
    ),
    (
        '20000000-0000-0000-0000-000000000012',
        '10000000-0000-0000-0000-000000000004',
        'use-news-responsibly',
        'Use News Responsibly',
        'Combine market news with a plan instead of reacting to headlines alone.',
        $$News can explain why a market is moving, but a headline is not a complete trade thesis. Check the source, publication time, affected symbols, and whether the information is already reflected in price.

When an important event is near, reduce exposure or stay out if your plan cannot handle the uncertainty. Protecting capital is a valid trading decision.$$,
        3,
        10,
        TRUE
    )
ON CONFLICT (id) DO UPDATE SET
    module_id = EXCLUDED.module_id,
    slug = EXCLUDED.slug,
    title = EXCLUDED.title,
    summary = EXCLUDED.summary,
    content = EXCLUDED.content,
    lesson_order = EXCLUDED.lesson_order,
    estimated_minutes = EXCLUDED.estimated_minutes,
    is_published = EXCLUDED.is_published,
    updated_at = CURRENT_TIMESTAMP;
