# Diagrams

## Architecture

`flview` has three user-facing modes (TUI, Web, Site) that share filesystem + preview helpers, which allows to use inside a K8S pod to check https and view file.

```mermaid
flowchart LR
    User((User))

    subgraph App["flview - single binary"]
        Main[src/main.rs<br/>CLI parsing + dispatch]

        TUI[src/tui.rs<br/>Terminal UI]
        Web[src/web.rs<br/>Axum web server]
        Site[src/site.rs<br/>HTTP/S fetch + headers/TLS info]

        FSU[src/fsutil.rs<br/>directory listing + file reads]
        Hex[src/hex.rs<br/>hex formatting]
    end

    subgraph External[External systems]
        FS[(Filesystem)]
        Net[(Network)]
        Browser((Browser))
        Term((Terminal))
    end

    User --> Main
    Main --> TUI
    Main --> Web
    Main --> Site

    TUI --> Term
    Web --> Browser

    TUI --> FSU
    Web --> FSU
    Site -.-> FSU
    TUI --> Hex
    Web --> Hex

    FSU --> FS
    Site --> Net
```

Notes:
- `src/main.rs` is the single entrypoint and selects the mode based on the subcommand.
- `src/fsutil.rs` and `src/hex.rs` are shared helpers used by both TUI and Web; `site` is network-only.
- `site --resolveto <IPV4>` overrides the TCP connect addresss but keeps the URL host for HTTP `Host` and (for HTTPS) SNI/cert validation.

## Program Flow

```mermaid
flowchart TD
    Start([Start: main.rs]) --> ParseArgs[Parse CLI arguments]
    ParseArgs --> ChooseRoot{--root provided?}
    ChooseRoot -- Yes --> RootArg[Use provided root]
    ChooseRoot -- No --> RootCwd[Use current directory]
    RootArg --> Dispatch{Subcommand?}
    RootCwd --> Dispatch

    %% TUI Branch - Blue theme
    Dispatch -- Tui --> TuiRun["tui::run(root)"]
    TuiRun --> CanonTui["fsutil::canonicalize_root"]
    CanonTui --> InitTUI[Enable raw mode + alt screen]
    InitTUI --> LoadDirTui["fsutil::list_dir(cwd)"]
    LoadDirTui --> TuiLoop[Main event loop]
    TuiLoop --> DrawUI[Draw UI]
    TuiLoop --> HandleInput{Key press?}

    HandleInput -- "Up/Down" --> UpdateSelection[Move selection]
    HandleInput -- "Enter" --> OpenSelected[Open dir / preview file]
    HandleInput -- "Backspace" --> GoUp[Go to parent dir]
    HandleInput -- "j/k" --> ScrollPreview["Scroll preview (if file open)"]
    HandleInput -- "PgUp/PgDn" --> PagePreview[Page preview]
    HandleInput -- "h/l" --> HScroll["Horizontal scroll (text preview)"]
    HandleInput -- "t" --> ModeText[Set preview mode: text]
    HandleInput -- "x" --> ModeHex[Set preview mode: hex]
    HandleInput -- "q" --> RestoreTerm[Restore terminal]

    UpdateSelection --> TuiLoop
    ScrollPreview --> TuiLoop
    PagePreview --> TuiLoop
    HScroll --> TuiLoop
    ModeText --> TuiLoop
    ModeHex --> TuiLoop

    OpenSelected --> OpenType{Selected is dir?}
    OpenType -- Yes --> ListDirTui["fsutil::list_dir(new cwd)"]
    ListDirTui --> TuiLoop
    OpenType -- No --> ReadFileTui["fsutil::read_file_limited"]
    ReadFileTui --> RenderMode{Preview mode?}
    RenderMode -- Text --> RenderTextTui[Render text preview]
    RenderMode -- Hex --> FormatHexTui["hex::format_hex_lines"]
    RenderTextTui --> TuiLoop
    FormatHexTui --> TuiLoop

    GoUp --> ListDirTui
    RestoreTerm --> Exit([Exit])

    %% Web Branch - Green theme
    Dispatch -- Web --> WebRun["web::run(root, addr)"]
    WebRun --> CanonWeb["fsutil::canonicalize_root"]
    CanonWeb --> SetupRouter[Setup Axum routes]
    SetupRouter --> BindAddr[Bind TCP listener]
    BindAddr --> Serve["axum::serve"]
    Serve --> Request{Incoming request}

    Request -- "GET /?path=..." --> Index[index]
    Index --> ResolveIndex["fsutil::resolve_under_root"]
    ResolveIndex --> MetaIndex["tokio::fs::metadata"]
    MetaIndex -- "dir" --> ListDirWeb[fsutil::list_dir]
    ListDirWeb --> RenderDir[Render dir HTML]
    MetaIndex -- "file" --> RenderFileIndex["Render file page (default text)"]

    Request -- "GET /view?path=...&mode=..." --> View[view_file]
    View --> ResolveView["fsutil::resolve_under_root"]
    ResolveView --> ReadFileWeb["fsutil::read_file_limited (spawn_blocking)"]
    ReadFileWeb -- "mode=text" --> RenderTextWeb[Render text HTML]
    ReadFileWeb -- "mode=hex" --> FormatHexWeb["hex::format_hex_lines"]
    FormatHexWeb --> RenderHexWeb[Render hex HTML]

    Request -- "GET /download?path=..." --> Download[download]
    Download --> ResolveDl["fsutil::resolve_under_root"]
    ResolveDl --> StreamFile[Stream file]

    Request -- "POST /upload?path=..." --> Upload[upload]
    Upload --> ResolveUp["fsutil::resolve_under_root"]
    ResolveUp --> SaveFile[Write file]
    SaveFile --> Redirect[Redirect to /?path=...]

    ResolveIndex -. "bad path" .-> Error400[400 Bad Request]
    ResolveView -. "bad path" .-> Error400
    ResolveDl -. "bad path" .-> Error400
    ResolveUp -. "bad path" .-> Error400

    RenderDir --> Response[HTTP response]
    RenderFileIndex --> Response
    RenderTextWeb --> Response
    RenderHexWeb --> Response
    StreamFile --> Response
    Redirect --> Response
    Error400 --> Response

    %% Shared utilities - Gray theme
    subgraph SharedUtils[Shared utilities]
        direction TB
        FSUtil[fsutil.rs<br/>canonicalize_root + list_dir + read_file_limited]
        HexUtil[hex.rs<br/>format_hex_lines]
        FS[(Filesystem)]
    end

    CanonTui -.-> FSUtil
    LoadDirTui -.-> FSUtil
    ListDirTui -.-> FSUtil
    ReadFileTui -.-> FSUtil
    CanonWeb -.-> FSUtil
    ResolveIndex -.-> FSUtil
    ResolveView -.-> FSUtil
    ResolveDl -.-> FSUtil
    ResolveUp -.-> FSUtil
    ListDirWeb -.-> FSUtil
    ReadFileWeb -.-> FSUtil

    FormatHexTui -.-> HexUtil
    FormatHexWeb -.-> HexUtil

    FSUtil --> FS

    %% Styling
    classDef tuiStyle fill:#4A90E2,stroke:#2E5C8A,color:#fff
    classDef webStyle fill:#7CB342,stroke:#558B2F,color:#fff
    classDef sharedStyle fill:#9E9E9E,stroke:#616161,color:#fff
    classDef decisionStyle fill:#FFA726,stroke:#F57C00,color:#fff

    class TuiRun,CanonTui,InitTUI,LoadDirTui,TuiLoop,DrawUI,UpdateSelection,OpenSelected,GoUp,ScrollPreview,PagePreview,HScroll,ModeText,ModeHex,ReadFileTui,RenderTextTui,FormatHexTui,RestoreTerm tuiStyle
    class WebRun,CanonWeb,SetupRouter,BindAddr,Serve,Index,View,Download,Upload,ResolveIndex,ResolveView,ResolveDl,ResolveUp,MetaIndex,ListDirWeb,ReadFileWeb,RenderDir,RenderFileIndex,RenderTextWeb,FormatHexWeb,RenderHexWeb,StreamFile,SaveFile,Redirect,Error400 webStyle
    class FSUtil,HexUtil,SharedUtils,FS sharedStyle
    class ChooseRoot,Dispatch,HandleInput,OpenType,RenderMode,Request decisionStyle
```
