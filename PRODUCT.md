# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop (native Rust app, egui; macOS / Windows / Linux). Not a web product; the impeccable web detector does not apply.

## Stack

Rust, egui/eframe (wgpu/glow GPU rendering), custom-drawn chart widgets. Single self-contained executable. Confirmed by the user.

## Users

Individual investors who hold or watch both US and Taiwan stocks and want to judge long-horizon performance (10+ years) by laying tickers over one another. Sit at a desktop for long sessions, switching between comparison and single-chart analysis.

## Product Purpose

A standalone, offline-capable clone of the TradingView Advanced Chart, centered on one job: compare the percentage return of many stocks (US and TW) over 10+ years on a shared baseline. No orders, no news.

## Positioning

Local, fast, tokenless. Real market data fetched with no API key or account, cached on disk, drawn natively at GPU speed even with 10+ years of daily bars on several symbols.

## Operating Context

Daily bars with dividend-adjusted closes (total return). TW symbols use `.TW`/`.TWO` suffixes, prices in TWD; US in USD. Percent-return comparison removes the currency difference by normalising each series to 0% at the shared start.

## Capabilities and Constraints

Confirmed scope for v1: multi-symbol % return overlay compare; candlestick chart with volume and indicators (MA/EMA, Bollinger, RSI, MACD); drawing tools (trend line, horizontal line, Fibonacci) and a watchlist; comparison statistics panel (total return, CAGR, max drawdown, volatility, Sharpe). Out of scope: order entry, news. Data from Yahoo Finance's public chart endpoint (unofficial, no token); must degrade gracefully when rate-limited or offline by using the disk cache.

## Evidence on Hand

Verified 2026-10-02: `query1.finance.yahoo.com/v8/finance/chart/{sym}?range=10y&interval=1d&events=div|split` returns ~2400-2500 daily bars with adjclose for both 2330.TW and AAPL, without any token. No fabricated data may be shipped.

## Product Principles

1. Real data only; never fabricate prices. Show source and freshness.
2. The comparison view is the home screen, not a secondary mode.
3. Speed is a feature: pan/zoom at display refresh rate, startup from cache instantly.
4. Dense, scannable, keyboard-friendly; the chart owns the screen.
5. Honest about total return vs price return, currency, and missing history.
