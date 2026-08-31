/// Compile-time-light, runtime-controlled debug macros.
///
/// Every macro expands to a `tracing::debug! / info! / warn! / error!` call
/// gated behind a per-module **target** string.  Targets are filtered at
/// runtime by the `EnvFilter` (built from `-v` / `-D` / config files /
/// `trace.set` RPC).  No custom `cfg` features required — everything is
/// always compiled so that AI-assisted debugging can toggle targets
/// dynamically without a rebuild.
///
/// ## Usage
///
/// ```ignore
/// mcc_dbg!("sem::fcall", "[FCALL] name={}", name);
/// mcc_dbg!(error, "[inst:{}] ERROR #{}", self.name, code);
/// mcc_dbg!(info, "[Pass 1] parsing {} files", count);
/// ```
///
/// ## Naming convention
///
/// | Prefix    | Meaning      | Default visibility        |
/// |-----------|-------------|---------------------------|
/// | `"sem::*"` | Semantic    | `debug` (needs `-vv`)     |
/// | `"inst::*"`| Instantiate | `debug` (needs `-vv`)     |
/// | `"parse::*"`| Parse      | `debug` (needs `-vv`)     |
/// | `"vec"`   | Vector      | `debug` (needs `-vv`)     |
/// | `"viz"`   | Viz/Layout  | `debug` (needs `-vv`)     |
/// | `"lsp::*"`| LSP/Query   | `debug` (needs `-vv`)     |
/// | `error`   | Always      | `error` (always visible)  |
/// | `warn`    | Always      | `warn`  (always visible)  |
/// | `info`    | Always      | `info`  (needs `-v`)      |

#[macro_export]
macro_rules! mcc_dbg {
    // ── Parse ──────────────────────────────────────────────
    ("parse::ast", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::parse::ast", $($arg)*);
    };
    ("parse::phrase", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::parse::phrase", $($arg)*);
    };

    // ── Semantic ────────────────────────────────────────────
    ("sem::fcall", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::fcall", $($arg)*);
    };
    ("sem::conds", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::conds", $($arg)*);
    };
    ("sem::class", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::class", $($arg)*);
    };
    ("sem::inst", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::inst", $($arg)*);
    };
    ("sem::module", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::module", $($arg)*);
    };
    ("sem::comp", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::sem::comp", $($arg)*);
    };

    // ── Instantiate / Pass2 ─────────────────────────────────
    ("inst::mod", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::mod", $($arg)*);
    };
    ("inst::comp", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::comp", $($arg)*);
    };
    ("inst::fcall", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::fcall", $($arg)*);
    };
    ("inst::points", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::points", $($arg)*);
    };
    ("inst::table", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::table", $($arg)*);
    };
    ("inst::dump", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::inst::dump", $($arg)*);
    };

    // ── Vector / Viz ────────────────────────────────────────
    ("vec", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::vec", $($arg)*);
    };
    ("viz", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::viz", $($arg)*);
    };

    // ── LSP / Query ─────────────────────────────────────────
    ("lsp::query", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::lsp::query", $($arg)*);
    };
    ("lsp::lapper", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::lsp::lapper", $($arg)*);
    };

    // ── Ref / Def resolution ────────────────────────────────
    ("refdef", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::refdef", $($arg)*);
    };
    ("refdef::chain", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::refdef::chain", $($arg)*);
    };

    // ── CLI / Config ────────────────────────────────────────
    ("build", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::build", $($arg)*);
    };
    ("config", $($arg:tt)*) => {
        tracing::debug!(target: "mcc::config", $($arg)*);
    };

    // ── Always-compiled levels ──────────────────────────────
    (error, $($arg:tt)*) => {
        tracing::error!(target: "mcc", $($arg)*);
    };
    (warn, $($arg:tt)*) => {
        tracing::warn!(target: "mcc", $($arg)*);
    };
    (info, $($arg:tt)*) => {
        tracing::info!(target: "mcc", $($arg)*);
    };
}
