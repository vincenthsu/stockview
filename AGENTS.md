# StockView
先讀 `docs/ARCHITECTURE.md`（模組地圖、型別、流程、檔案位置），再只開需要的檔案；勿通讀 src/。
- 產品：`PRODUCT.md`；視覺：`DESIGN.md`。
- 驗證：`cargo test`；視覺用 `STOCKVIEW_SHOT=/path.png cargo run --release`。
- 改動結構（新增模組/指標/警示條件/持久化欄位）後，同步更新 `docs/ARCHITECTURE.md`。
