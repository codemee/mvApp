[English](DEVELOPMENT.en.md) | **繁體中文**

# mvApp 開發與技術說明

一般使用說明請見 [README](../README.md)。本文件給想建置、修改或了解內部運作的開發者。

## 環境與建置

- 平台：僅限 Windows（目標 `x86_64-pc-windows-msvc`）。
- 工具：Rust stable（edition 2024），以及 Visual Studio Build Tools（含 Windows SDK，建置時需要資源編譯器來編譯 `app.rc`）。
- 主要相依套件：`eframe`／`egui` 0.37（介面）、`windows` 0.62（Win32 API）、`tray-icon` 0.26（系統匣）、`raw-window-handle` 0.6、`embed-resource`（建置時嵌入資源）。

| 指令 | 用途 |
|---|---|
| `cargo run` | 以 debug 模式執行（會附帶主控台，方便看輸出） |
| `cargo run --release` | 以 release 模式執行（Windows 子系統，沒有主控台） |
| `cargo build --release` | 建置發布版，輸出 `target\release\mvapp.exe`（約 10 MB） |
| `cargo test` | 執行測試（見〈測試〉） |

### 發布版的建置設定

- `.cargo/config.toml` 設定 `-C target-feature=+crt-static`，**靜態連結 VC++ 執行階段**，所以 exe 在沒有安裝 VC++ Redistributable 的電腦上也能直接執行。
- `Cargo.toml` 的 `[profile.release]` 使用 `lto = true`、`codegen-units = 1`、`strip = true`、`panic = "abort"`，縮小檔案大小。
- `build.rs` 透過 `embed-resource` 編譯 `app.rc`，內容有兩項：
  - 資源 1：`app.manifest`，宣告 PerMonitorV2 的 DPI 感知。
  - 資源 2：`assets/icon.ico`（16～256 多尺寸，每個尺寸原生繪製）。系統匣圖示會以序號 2 直接載入，才能取得正確的小尺寸圖像。

## 專案結構

| 檔案 | 職責 |
|---|---|
| `src/main.rs` | 進入點：單一執行個體檢查、圖形後端設定、載入中文字型、建立視窗、記下主視窗 HWND |
| `src/app.rs` | 介面：工具列、螢幕窗格、拖放、狀態列、配色與樣式、啟動與關閉的視窗行為 |
| `src/monitors.rs` | 螢幕列舉（位置、工作區、DPI） |
| `src/windows_list.rs` | 視窗列舉與過濾 |
| `src/icons.rs` | 取得視窗圖示並轉成 egui texture（含快取） |
| `src/mover.rs` | 搬移視窗的座標換算與 Win32 呼叫 |
| `src/toolbar.rs` | 工具列圖示按鈕與以 painter 繪製的圖示 |
| `src/tray.rs` | 系統匣圖示、右鍵選單、顯示／隱藏主視窗 |
| `src/single_instance.rs` | 單一執行個體（具名 Mutex／Event） |
| `src/autostart.rs` | 開機自動執行（寫入登錄檔） |
| `src/settings.rs` | 設定檔的讀寫 |
| `src/i18n.rs` | 介面文字翻譯與系統語言偵測 |
| `assets/` | 程式圖示（`icon.png`、`icon.ico`） |

## 運作原理

### 螢幕與視窗列舉

- 螢幕：`EnumDisplayMonitors` + `GetMonitorInfoW`，取得完整範圍、工作區、是否為主要螢幕，並用 `GetDpiForMonitor` 取得 DPI；依左邊界、上邊界排序，讓窗格的順序和實體排列一致。
- 視窗：`EnumWindows` 之後，下列視窗會被排除：
  - 不可見的視窗、有擁有者（owner）的視窗。
  - 具有 `WS_EX_TOOLWINDOW` 或 `WS_EX_NOACTIVATE` 且沒有 `WS_EX_APPWINDOW` 的視窗。
  - 被 DWM「cloak」的視窗（例如其他虛擬桌面上的 UWP 視窗）。
  - 沒有標題的視窗、桌面外殼視窗（`Progman`、`WorkerW`）、以及 mvApp 自己的視窗。
- 每個視窗所在的螢幕由 `MonitorFromWindow(..., MONITOR_DEFAULTTONEAREST)` 決定。
- 介面每 600 ms 重新列舉一次；拖曳進行中暫停，避免清單抖動。

### 圖示取得（`icons.rs`）

依序嘗試：

1. **UWP／封裝應用程式**（只對由 `ApplicationFrameHost.exe` 代管的視窗）：用 `SHGetPropertyStoreForWindow` 讀出視窗的 AppUserModelID，再向 shell 要 `shell:AppsFolder\<AUMID>` 的圖像（`IShellItemImageFactory`）。否則會取到代管程式本身的通用圖示。
2. 視窗圖示：`WM_GETICON`（大、小圖示），再退到類別圖示 `GCLP_HICON`。
3. 執行檔圖示：`SHGetFileInfoW`。

`HICON` 轉成 RGBA 時，沒有 alpha 通道的舊式圖示會改用遮罩補上；shell 回傳的預乘 alpha 位元圖會還原成非預乘。結果以 HWND 為 key 快取成 texture，視窗消失時才移除。

### 搬移視窗（`mover.rs`）

- 座標換算 `map_rect`：把視窗的位置與大小，依「來源螢幕工作區 → 目標螢幕工作區」的比例換算，再夾限在目標工作區內。這個函式是純函式，有單元測試。
- 流程：
  - 一般視窗：`GetWindowRect` → 換算 → `SetWindowPos`。
  - 最大化的視窗：先還原、搬移，再最大化。
  - 最小化的視窗：先還原、搬移，再最小化（因此「先最大化再最小化」的視窗搬完會變成一般狀態）。
- 跨 DPI 的螢幕之間，系統可能在第一次移動後又調整大小，所以 `SetWindowPos` 會呼叫兩次。
- 失敗（常見原因是目標視窗以系統管理員身分執行，被 UIPI 擋下）會以 `MoveError` 回傳，由介面依語言顯示訊息，不會讓程式當掉。

### 介面與拖放（`app.rs`）

- 使用 egui 的 `dnd_drag_source`／`dnd_drop_zone`，拖放的 payload 是視窗 HWND。放開時若目標螢幕與視窗目前所在螢幕不同，才會呼叫搬移。
- 版面：寬度夠時（每個窗格至少 300 px）窗格左右並排、等寬；否則改為上下排列，整體可捲動。
- 注意：`dnd_drop_zone` 在拖曳期間會改用 `widgets.inactive`／`widgets.active` 的樣式重繪放置區，所以 `apply_styles` 刻意把這兩組樣式設成「靜止時與窗格一致、可放置時帶強調色外框」，拖放回饋才會正確。
- 狀態列的訊息以資料（`Status`）保存，而不是文字，這樣切換語言時能即時重新翻譯。

### 系統匣與視窗生命週期（`tray.rs`、`app.rs`）

- 啟動時只顯示在系統匣。但 eframe 會在第一幀畫完後無條件呼叫 `set_visible(true)`，所以 `with_visible(false)` 擋不住。做法是：
  1. 視窗建立後立即把它設成 `WS_EX_LAYERED` 且 alpha 為 0（全透明）。
  2. 等到第二個影格，用 Win32 `ShowWindow(SW_HIDE)` 隱藏，並還原不透明。
  3. 這樣使用者不會看到視窗閃一下。
- 顯示／隱藏都直接用 Win32 操作主視窗 HWND，而不是 egui 的 `ViewportCommand`，因為系統匣事件可能在 egui 更新迴圈之外發生，隱藏中的視窗也不會被重繪。
- 按下關閉鈕：攔截 `close_requested`，送出 `CancelClose` 並隱藏視窗。
- 右鍵選單「結束」：設定旗標、對主視窗送 `WM_CLOSE` 走正常關閉流程；若視窗隱藏時沒有被處理，1.5 秒後強制結束行程。
- 若系統匣圖示建立失敗，程式會直接顯示主視窗，且關閉視窗就是結束程式，避免使用者找不到它。

### 單一執行個體（`single_instance.rs`）

- 啟動最早期建立具名 Mutex `Local\mvApp.SingleInstance`；若已存在（`ERROR_ALREADY_EXISTS`），代表已有實例。
- 第一個實例同時建立自動重設的具名 Event `Local\mvApp.ShowWindow`，並開一條背景執行緒等待它。第二個實例只要 `SetEvent` 就結束，第一個實例收到後會顯示主視窗。
- 名稱前綴 `Local\` 表示只限目前的登入工作階段，不同使用者各自可以執行一份。

### 開機自動執行（`autostart.rs`）

- 在 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 新增名為 `mvApp` 的值，內容是加上引號的目前執行檔完整路徑。只寫入目前使用者的機碼，不需要系統管理員權限。
- 啟動時若發現已啟用、但路徑和目前執行檔不同（例如檔案被搬動過），會自動更新成目前位置。
- 系統匣選單的勾選狀態以登錄檔實際內容為準。

### 設定與多語系（`settings.rs`、`i18n.rs`）

- 設定檔位於 `%APPDATA%\mvapp\settings.txt`，內容是兩行 `key=value`：

  ```
  theme=system|light|dark
  lang=system|zh|en
  ```

  只有使用者切換時才會寫入；寫入失敗時靜默略過（設定不是關鍵功能）。
- 語言偏好為「隨系統」時，以 `GetUserDefaultLocaleName` 判斷：`zh` 開頭視為繁體中文介面，其他一律英文。
- 所有介面文字集中在 `i18n.rs` 的 `Tr`；搬移失敗等訊息以列舉（`MoveError`）傳遞，顯示時才翻譯。
- 中文字型：啟動時依序嘗試載入 `C:\Windows\Fonts` 下的 `msjh.ttc`、`msyh.ttc`、`meiryo.ttc` 作為備援字型，否則視窗標題會顯示成方塊。

### 配色與樣式

- 風格參考 Windows 11 Fluent：中性底色、細框群組（標題嵌在框線上）、內部白色清單區、灰色細線條圖示。
- 配色定義在 `app.rs` 的 `Palette`：淺色取自鋼彈 RX-78-2（藍、紅、黃、灰），深色取自新世紀福音戰士初號機（暗紫、螢光綠、橘）。大面積使用中性色，鮮豔色只用在各螢幕的圖示與拖放高亮。
- 淺色／深色各自設定一次 egui 樣式，切換時由 egui 依主題套用。
- 工具列規格：高度 42、內距四邊 4、按鈕 34×34、圖示畫布 20×20 置中、按鈕間距 4；一般按鈕圓角 4、狀態按鈕圓角 8；分隔線寬 1，上下外距 5、左右外距 4。工具列圖示全部以 painter 繪製（`toolbar.rs`），不依賴圖檔。

### 圖形後端

Intel 部分驅動的 Vulkan 元件（`igvk64.dll`）在 wgpu 預設使用 Vulkan 時會讓程式一啟動就崩潰。因此 `main.rs` 預設把 `WGPU_BACKEND` 設為 `dx12`；若使用者自行設定了這個環境變數，則尊重其設定。

## 測試

```powershell
cargo test
```

- `mover`：座標換算的單元測試（同尺寸螢幕、不同尺寸螢幕、夾限）。
- `smoke`：螢幕、視窗、圖示列舉的煙霧測試。這些需要在有桌面的登入工作階段中執行。
- `autostart`：實際寫入再刪除 `HKCU` 的 `Run` 值。若執行時使用者已啟用開機自動執行，測試會略過，避免動到真實設定。

## 發布流程

目前的發布在**本機**進行，沒有設定 CI：

1. 更新 `Cargo.toml` 的版本號。
2. `cargo build --release`，並確認 exe 可正常啟動、沒有引用 VC++ 動態函式庫。
3. 提交並推送到 `main`。
4. 計算 exe 的 SHA-256，寫入 release 說明（中英雙語）。
5. 用 `gh release create vX.Y.Z target\release\mvapp.exe --notes-file ...` 建立 release 並上傳 exe。
6. 從公開網址重新下載，比對 SHA-256 是否一致。

## 程式圖示

`assets/icon.png`、`assets/icon.ico` 是以 PowerShell 與 `System.Drawing` 繪製的：每個尺寸（16、20、24、32、40、48、64、128、256）各自原生繪製，小尺寸的線條較粗，避免縮小後線條斷裂。箭頭放在視窗內容區（標題列以下）的下半部，和標題列保持距離，讓畫面單純。整體垂直位置經過量測微調（外框中心約 49.6%、像素重心約 50.5%，都在畫布中央 ±1% 內），所以在工作列上看起來垂直置中；只看外框置中是不夠的，要連像素重心一起看，視覺上才不會偏高或偏低。產生圖示的腳本沒有納入版本控制。

## 授權

GPL-3.0-or-later，全文見 [LICENSE](../LICENSE)。
