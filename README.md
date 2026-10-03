# StockView

Cross-platform standalone stock chart in Rust (egui). Compare 10+ year % returns of US and Taiwan stocks, candlestick charts with indicators and drawing tools. Data: Yahoo Finance public chart endpoint, no API token; cached on disk.

    cargo run --release

Compare mode: type a ticker (2330, AAPL) in the search box. K 線 mode: MA/EMA/Bollinger/RSI/MACD, trend line, horizontal line, Fibonacci. State saved automatically.

個股基本面：工具列「基本面」可查看個股本益比、淨值比、EPS、殖利率、ROE、營收與盈餘年增率、淨利率、負債權益比、營收、淨利、現金、負債、自由現金流及市值；顯示財報季末、資料取得時間與幣別。資料來源 Yahoo Finance，24 小時磁碟快取，缺值顯示「—」；ETF／指數不適用。

基本面選股：工具列「選股」，選擇全部自選股、台股／美股預設清單，或輸入自訂代號（非全市場掃描）。數字代號預設 `.TW`，上櫃使用 `.TWO`。啟用並設定本益比、淨值比、殖利率、ROE、成長率、淨利率或負債權益比範圍，可搭配產業關鍵字；所有條件須同時符合，缺少啟用指標、過期或更新失敗的資料不列入結果。可排序、查看基本面、開啟 K 線或加入自選；「更新資料並選股」強制刷新。
