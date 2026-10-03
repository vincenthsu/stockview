# StockView 架構（agent 先讀這份，再按需開檔）

Rust 2024 / eframe+egui 0.36 桌面 app，單執行檔。約 5.5k 行 `src/`。產品背景見 `PRODUCT.md`，視覺規範見 `DESIGN.md`。
指令：`cargo run --release`、`cargo test`（29 個離線測試，另 1 個 ignored Yahoo 基本面連線測試）。

## 模組地圖（src/）
| 檔案 | 行數 | 職責 | 依賴 |
|---|---|---|---|
| main.rs | 25 | 宣告 mod、開 eframe 視窗(1480x900) | app |
| app.rs | 2380 | `App`：全部 UI、狀態、背景請求、警示輪詢、回測視窗、設定匯入匯出 | 全部 |
| data.rs | 216 | `Bar/Series/SearchHit`、Yahoo 抓取、磁碟快取、cache/config 目錄 | alert::Quote |
| fundamentals.rs | 272 | Yahoo 基本面、記憶體 cookie/crumb 工作階段、24 小時獨立磁碟快取、指標與區間篩選 | data |
| fundamental_ui.rs | 343 | 個股基本面／候選池選股視窗、背景序列載入、缺值與過期資料排除、結果排序 | fundamentals |
| store.rs | 290 | `Persisted`（state.json）、`Range/Interval/Mode/Drawing/Group`、匯入匯出 | alert, indicator |
| calc.rs | 463 | 純數值：sma/ema/bollinger/rsi/macd/kd/cci/willr/atr/obv/roc/mfi/adx/bias/sar/hma/wma、`resample`、`stats` | data::Bar |
| indicator.rs | 398 | 指標設定層：`Kind/Cfg/Out/Ln/Param`；`Cfg::compute(bars)->Out` | calc |
| candle.rs | 755 | K 線圖繪製+互動（`show`）、畫線工具、副圖、標記 | axis, indicator, theme, compare::chip |
| compare.rs | 395 | 多檔 % 報酬疊圖（`show`）、末端排名標籤、decimate | axis, theme |
| axis.rs | 252 | 日期/價格/百分比/成交量格式化、nice/log/time ticks、標籤避撞 `spread` | calc::DAY |
| alert.rs | 415 | 警示條件 `Cond`/`CondKind`、`series()` 回 bool 序列、`should_fire`、`merge_live` | calc, data, indicator |
| backtest.rs | 244 | 策略 `Strat`(KD/RSI/MA 交叉)、`signals`、`run -> Report` | calc |
| mail.rs | 89 | `SmtpCfg`（獨立檔存於 config_dir）、`send`（lettre） | — |
| theme.rs | 197 | `Palette::new(dark, red_up)`、`apply`、`install_fonts`(CJK) | — |

依賴方向：app → {candle, compare, alert, backtest, store, data, mail, theme, fundamental_ui}；fundamental_ui → fundamentals → data；candle/compare → axis/theme/indicator；indicator/alert/backtest → calc。calc/axis/theme 為葉節點。

## 核心資料型別
- `data::Bar {t(unix s UTC), o,h,l,c, adj(還原收盤), v}`；`Series {symbol,name,currency,exchange,bars,fetched_at}`。
- `store::Persisted`（`#[serde(default)]`，缺欄位容錯）：groups(自選群組)/active_group/compare/selected/mode/range+custom_from/to/interval/dark/red_up/total_return/log_scale/indicators/drawings(by symbol)/colors(symbol→色索引)/names/alerts/alert_log。舊版 `watchlist` 載入時遷移進 `groups`，不回寫。`normalize()` 修復壞索引。
- `store::Drawing` = Trend / HLine / Fib（以 時間t+價格p 儲存，與縮放無關）。
- `indicator::Cfg {kind, params, color, enabled}`；`Out` 含多條 `Ln`（線/柱/點、overlay 或副圖、`is_band`）。新增指標：加 `Kind` 變體 + `OVERLAYS/PANES` + `label/short/params` + `compute` 分支 + `calc` 函式。
- `alert::Alert {id,symbol,cond,repeat(Once|EveryMinute),toast,system,email,enabled,chart,last_fired,fired}`；`AlertEvent` 為歷史紀錄。

## 執行流程（app.rs `ui()` 每幀）
`poll`(收 mpsc `Msg`) → `debug_shot` → `alert_poll` → 快捷鍵(Cmd+Z 撤銷畫線、拖入 .json 匯入) → 面板：top_bar / 右 watchlist(264px) / 比較模式底部 stats_panel、K 線模式左 tools_panel / `central`(compare::show 或 candle::show) → alerts_window、backtest_window、toasts、notice → 防抖存檔（dirty 後 >800ms，`on_exit` 亦存）。

背景工作：`request/send_search/…` 以 thread 呼叫 `data::*`，結果經 `Msg::{Series,Hits,Quote,Mail}` 回主執行緒；`loading/errors/quoting` 集合追蹤進度。`series: HashMap<sym, Arc<Series>>`；`bars_cache` 快取當前 K 線（sym,Interval,total_return）重採樣結果；`mark_cache` 快取警示圖上標記。

警示：`alert_poll` 每輪詢（`last_quote_poll`）抓 `fetch_quote` → `eval_alerts`（`merge_live` 併入即時價→`Cond::series` 取最後值→`should_fire`；`armed` 記上次狀態，Once 只在 false→true 觸發，首次只設基準）→ `fire_alert`（toast / `system_notify`(macOS) / 郵件 Msg::Mail、寫 alert_log）。

回測：`backtest_window` 取 `chart_bars()` → `backtest::run(strat, params, bars, years, cost, total_return)` → `Report`（績效+權益曲線+交易列表）。

基本面：`fundamental_ui::State::poll` 用獨立 mpsc 序列載入候選池，每筆間隔 500ms；Yahoo 工作階段錯誤／429 停止排隊。`windows` 回傳 `Action::{Chart,Add}`，由 App 開啟 K 線／加入自選。選股套用全部啟用的區間條件及產業關鍵字，缺值／過期／更新失敗者排除；範圍是全部自選、台美股預設清單或自訂代號，非全市場。

## 資料與檔案
- 來源：Yahoo `query1.finance.yahoo.com/v8/finance/chart/{sym}`（10y→全期、1d、含 div/split，無 token）；搜尋與報價亦走 Yahoo（`fetch_series/fetch_quote/search`）。台股 `.TW/.TWO`。
- 快取：`ProjectDirs("dev","stockview","StockView").cache_dir()/{safe symbol}.json`；`is_fresh` = 6 小時。離線/限流退回快取。
- 基本面：Yahoo `query2.finance.yahoo.com/v10/finance/quoteSummary/{sym}`，使用 fc.yahoo.com cookie + getcrumb（僅留記憶體）；`fundamentals-{safe symbol}.json`，24 小時快取。指標百分比統一為百分點，Yahoo debtToEquity 已是百分點；財報金額與市值分別使用財報／股價幣別。
- 設定：`config_dir()/state.json`（Persisted）、SMTP 設定另檔（mail.rs `path()`）。
- 匯出入：`Persisted::export_json/import_json`，含 `EXPORT_APP="stockview"`、`EXPORT_VERSION=1` 檢查。

## 開發輔助環境變數（app.rs `debug_shot`）
`STOCKVIEW_SHOT=<png>` 載入完成後截圖並退出（同時 `no_save` 不寫 state）；`STOCKVIEW_MODE=chart|compare`、`STOCKVIEW_DARK=1`、`STOCKVIEW_IND=all`、`STOCKVIEW_ALERTS=1|window`、`STOCKVIEW_RANGE=<…>`。
`STOCKVIEW_FUNDAMENTALS=1` 開啟基本面；`STOCKVIEW_SCREENER="2330 AAPL JPM"` 開啟指定候選池並選股；截圖會等待基本面載入。

## 慣例
- UI 文字為正體中文；數字用等寬字型；台股預設紅漲綠跌(`red_up`)。
- 圖表為自繪（egui Painter），view 以 `(f64,f64)` 表索引/時間視窗；`candle::idx_of_t/t_of_idx` 在時間與 bar 索引互轉。
- 錯誤以 `Result<_, String>` 傳遞；不偽造資料。
- 測試放各檔 `#[cfg(test)]`，無整合測試。
