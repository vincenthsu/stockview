# StockView

Cross-platform standalone stock chart in Rust (egui). Compare 10+ year % returns of US and Taiwan stocks, candlestick charts with indicators and drawing tools. Data: Yahoo Finance public chart endpoint, no API token; cached on disk.

    cargo run --release

Compare mode: type a ticker (2330, AAPL) in the search box. K 線 mode: MA/EMA/Bollinger/RSI/MACD, trend line, horizontal line, Fibonacci. State saved automatically.
