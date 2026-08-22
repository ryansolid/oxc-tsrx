//! The canonical Oxfmt formatting lane and the project-owned options it is driven by.

use std::{error::Error, fmt, str::FromStr, time::Instant};

use oxc_allocator::Allocator;
use oxc_formatter::{
    ArrowParentheses, AttributePosition, BracketSameLine, BracketSpacing, CommentLineStrategy,
    EmbeddedLanguageFormatting, Expand, JsFormatOptions, JsdocOptions, LineWrappingStyle,
    QuoteProperties, QuoteStyle, Semicolons, TrailingCommas, format_program, parse_for_format,
};
use oxc_formatter_core::{IndentStyle, IndentWidth, LineEnding, LineWidth};
use serde::Deserialize;
use serde_json::Value;

use super::timings::{FormatEngineTimings, elapsed_ns};
use crate::{DynamicTagContract, DynamicTagError, SourceKind, validate_dynamic_tags};

/// Why one canonical Oxfmt formatting pass produced no output.
#[derive(Debug)]
pub enum FormatError {
    /// Canonical OXC could not parse the projected source. Holds its joined diagnostic text.
    Parse { detail: String },
    /// Canonical Oxfmt built a document it could not print. Holds its wording.
    Print { detail: String },
    /// One of the caller's Oxfmt options is not usable.
    Options(FormatOptionError),
    /// The TSRX dynamic-tag scaffold did not survive the parse.
    DynamicTags(DynamicTagError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { detail } => write!(formatter, "OXC formatter parse failed: {detail}"),
            Self::Print { detail } => write!(formatter, "OXC formatter print failed: {detail}"),
            Self::Options(error) => error.fmt(formatter),
            Self::DynamicTags(error) => error.fmt(formatter),
        }
    }
}

impl Error for FormatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Options(error) => Some(error),
            Self::DynamicTags(error) => Some(error),
            Self::Parse { .. } | Self::Print { .. } => None,
        }
    }
}

impl From<FormatOptionError> for FormatError {
    fn from(error: FormatOptionError) -> Self {
        Self::Options(error)
    }
}

impl From<DynamicTagError> for FormatError {
    fn from(error: DynamicTagError) -> Self {
        Self::DynamicTags(error)
    }
}

/// A single Oxfmt option this adapter refuses to pass to canonical Oxfmt.
///
/// `detail` quotes canonical Oxfmt's own wording where the option is parsed upstream, so the
/// rendered text stays the one users already see; see [`ConfigError`](super::ConfigError) for why
/// the upstream error type itself is not carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatOptionError {
    /// A numeric option outside the range canonical Oxfmt accepts.
    Numeric { option: &'static str, value: u16, detail: String },
    /// A named option canonical Oxfmt parses and rejected, with its wording.
    Named { option: &'static str, value: String, detail: String },
    /// A named option this adapter resolves itself, so there is no upstream wording to quote.
    Unrecognized { option: &'static str, value: String },
}

impl fmt::Display for FormatOptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric { option, value, detail } => {
                write!(formatter, "invalid Oxfmt {option} {value}: {detail}")
            }
            Self::Named { option, value, detail } => {
                write!(formatter, "invalid Oxfmt {option} `{value}`: {detail}")
            }
            Self::Unrecognized { option, value } => {
                write!(formatter, "invalid Oxfmt {option} `{value}`")
            }
        }
    }
}

impl Error for FormatOptionError {}

impl FormatOptionError {
    fn numeric(option: &'static str, value: u16, detail: impl fmt::Display) -> Self {
        Self::Numeric { option, value, detail: detail.to_string() }
    }

    fn named(option: &'static str, value: &str, detail: impl fmt::Display) -> Self {
        Self::Named { option, value: value.to_string(), detail: detail.to_string() }
    }

    fn unrecognized(option: &'static str, value: &str) -> Self {
        Self::Unrecognized { option, value: value.to_string() }
    }
}

#[derive(Debug)]
pub struct FormatRequest<'a> {
    pub parse_source: &'a str,
    pub source_kind: SourceKind,
    pub dynamic_tags: Option<DynamicTagContract<'a>>,
    pub options: Option<&'a FormatOptions>,
}

/// Oxfmt-compatible options that affect JavaScript, TypeScript, JSX, and TSRX output.
///
/// This project-owned representation keeps revision-specific OXC option types inside this adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormatOptions {
    pub use_tabs: Option<bool>,
    pub tab_width: Option<u8>,
    pub end_of_line: Option<String>,
    pub print_width: Option<u16>,
    pub single_quote: Option<bool>,
    pub jsx_single_quote: Option<bool>,
    pub quote_props: Option<String>,
    pub trailing_comma: Option<String>,
    pub semi: Option<bool>,
    pub arrow_parens: Option<String>,
    pub bracket_spacing: Option<bool>,
    pub bracket_same_line: Option<bool>,
    pub object_wrap: Option<String>,
    pub single_attribute_per_line: Option<bool>,
    pub embedded_language_formatting: Option<String>,
    pub html_whitespace_sensitivity: Option<String>,
    /// Oxfmt's `jsdoc` option, parsed by [`FormatOptions::set_jsdoc`].
    pub jsdoc: Option<JsdocSetting>,
}

/// Whether one configuration scope turns `jsdoc` comment formatting on, and with which
/// sub-options.
///
/// Canonical Oxfmt spells this option `true`, `false`, or an object. The disabled form stays a
/// value rather than an absent option so one `overrides` entry can turn the option back off for
/// the files it matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsdocSetting {
    Disabled,
    Enabled(JsdocConfig),
}

/// The sub-options canonical Oxfmt reads out of a `jsdoc` object.
///
/// Every field stays `None` when the author did not write it, so the pinned formatter's own
/// defaults decide. The two string fields are validated where every other named option is, in
/// [`js_format_options`], and unknown fields are refused rather than silently ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JsdocConfig {
    pub capitalize_descriptions: Option<bool>,
    pub description_with_dot: Option<bool>,
    pub add_default_to_description: Option<bool>,
    pub prefer_code_fences: Option<bool>,
    pub line_wrapping_style: Option<String>,
    pub comment_line_strategy: Option<String>,
    pub separate_tag_groups: Option<bool>,
    pub separate_returns_from_param: Option<bool>,
    pub bracket_spacing: Option<bool>,
    pub description_tag: Option<bool>,
    pub keep_unparsable_example_indent: Option<bool>,
}

impl FormatOptions {
    /// Reads Oxfmt's `jsdoc` option out of one JSON configuration value.
    ///
    /// Callers hand over the raw JSON because the parsed shape is this adapter's own, which keeps
    /// the `true | false | object` form canonical Oxfmt documents in the one module allowed to
    /// know how the pinned formatter spells it.
    ///
    /// # Errors
    ///
    /// Returns [`FormatOptionError`] when the value is neither a boolean nor an object of the
    /// sub-options canonical Oxfmt accepts.
    pub fn set_jsdoc(&mut self, value: &Value) -> Result<(), FormatOptionError> {
        let setting = match value {
            Value::Bool(false) => JsdocSetting::Disabled,
            Value::Bool(true) => JsdocSetting::Enabled(JsdocConfig::default()),
            Value::Object(_) => {
                JsdocSetting::Enabled(serde_json::from_value(value.clone()).map_err(|error| {
                    FormatOptionError::named("jsdoc", &value.to_string(), error)
                })?)
            }
            other => {
                return Err(FormatOptionError::named(
                    "jsdoc",
                    &other.to_string(),
                    "expected `true`, `false`, or an object of JSDoc options",
                ));
            }
        };
        self.jsdoc = Some(setting);
        Ok(())
    }
}

#[derive(Debug)]
pub struct EngineFormatResult {
    pub code: String,
    pub timings: FormatEngineTimings,
    pub parse_count: u32,
}

/// Formats one legal JavaScript/TypeScript projection with canonical Oxfmt.
///
/// This deliberately calls [`parse_for_format`] once and [`format_program`] once. Keeping this
/// sequence here prevents revision-specific OXC APIs from leaking into the TSRX language crates
/// and makes the one-parse invariant directly inspectable.
///
/// # Errors
///
/// Returns [`FormatError`] when an option is unusable, or when canonical OXC parsing, dynamic-tag
/// validation, or document printing fails.
pub fn format(request: &FormatRequest<'_>) -> Result<EngineFormatResult, FormatError> {
    let allocator = Allocator::default();
    let source_type = request.source_kind.source_type();

    let started = Instant::now();
    let parsed = parse_for_format(&allocator, request.parse_source, source_type);
    if !parsed.diagnostics.is_empty() {
        let detail =
            parsed.diagnostics.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        return Err(FormatError::Parse { detail });
    }
    validate_dynamic_tags(&parsed.program, request.dynamic_tags)?;
    let parse_ns = elapsed_ns(started);

    let started = Instant::now();
    let options =
        request.options.map_or_else(|| Ok(JsFormatOptions::default()), js_format_options)?;
    let code = format_program(&allocator, &parsed.program, options, None)
        .print()
        .map_err(|error| FormatError::Print { detail: error.to_string() })?
        .into_code();
    let format_ns = elapsed_ns(started);

    Ok(EngineFormatResult {
        code,
        timings: FormatEngineTimings { parse_ns, format_ns },
        parse_count: 1,
    })
}

fn js_format_options(options: &FormatOptions) -> Result<JsFormatOptions, FormatOptionError> {
    let mut resolved = JsFormatOptions::default();
    if let Some(use_tabs) = options.use_tabs {
        resolved.indent_style = if use_tabs { IndentStyle::Tab } else { IndentStyle::Space };
    }
    if let Some(width) = options.tab_width {
        resolved.indent_width = IndentWidth::try_from(width)
            .map_err(|error| FormatOptionError::numeric("tabWidth", u16::from(width), error))?;
    }
    if let Some(value) = &options.end_of_line {
        resolved.line_ending = LineEnding::from_str(value)
            .map_err(|error| FormatOptionError::named("endOfLine", value, error))?;
    }
    if let Some(width) = options.print_width {
        resolved.line_width = LineWidth::try_from(width)
            .map_err(|error| FormatOptionError::numeric("printWidth", width, error))?;
    }
    if let Some(single) = options.single_quote {
        resolved.quote_style = if single { QuoteStyle::Single } else { QuoteStyle::Double };
    }
    if let Some(single) = options.jsx_single_quote {
        resolved.jsx_quote_style = if single { QuoteStyle::Single } else { QuoteStyle::Double };
    }
    if let Some(value) = &options.quote_props {
        resolved.quote_properties = QuoteProperties::from_str(value)
            .map_err(|error| FormatOptionError::named("quoteProps", value, error))?;
    }
    if let Some(value) = &options.trailing_comma {
        resolved.trailing_commas = TrailingCommas::from_str(value)
            .map_err(|error| FormatOptionError::named("trailingComma", value, error))?;
    }
    if let Some(semi) = options.semi {
        resolved.semicolons = if semi { Semicolons::Always } else { Semicolons::AsNeeded };
    }
    if let Some(value) = &options.arrow_parens {
        resolved.arrow_parentheses = match value.as_str() {
            "avoid" => ArrowParentheses::AsNeeded,
            "always" => ArrowParentheses::Always,
            _ => {
                return Err(FormatOptionError::named(
                    "arrowParens",
                    value,
                    "expected `always` or `avoid`",
                ));
            }
        };
    }
    if let Some(spacing) = options.bracket_spacing {
        resolved.bracket_spacing = BracketSpacing::from(spacing);
    }
    if let Some(same_line) = options.bracket_same_line {
        resolved.bracket_same_line = BracketSameLine::from(same_line);
    }
    if let Some(value) = &options.object_wrap {
        resolved.expand = match value.as_str() {
            "preserve" => Expand::Auto,
            "collapse" => Expand::Never,
            _ => return Err(FormatOptionError::unrecognized("objectWrap", value)),
        };
    }
    if let Some(single_attribute) = options.single_attribute_per_line {
        resolved.attribute_position =
            if single_attribute { AttributePosition::Multiline } else { AttributePosition::Auto };
    }
    if let Some(value) = &options.embedded_language_formatting {
        resolved.embedded_language_formatting = EmbeddedLanguageFormatting::from_str(value)
            .map_err(|error| {
                FormatOptionError::named("embeddedLanguageFormatting", value, error)
            })?;
    }
    if let Some(value) = &options.html_whitespace_sensitivity {
        resolved.html_whitespace_sensitivity_ignore = match value.as_str() {
            "ignore" => true,
            "css" | "strict" => false,
            _ => return Err(FormatOptionError::unrecognized("htmlWhitespaceSensitivity", value)),
        };
    }
    if let Some(setting) = &options.jsdoc {
        resolved.jsdoc = jsdoc_options(setting)?;
    }
    Ok(resolved)
}

/// Maps the project-owned `jsdoc` option onto the pinned formatter's own options.
///
/// The two string sub-options quote canonical Oxfmt's wording for an invalid value, the way every
/// other named option here does.
fn jsdoc_options(setting: &JsdocSetting) -> Result<Option<JsdocOptions>, FormatOptionError> {
    let JsdocSetting::Enabled(config) = setting else {
        return Ok(None);
    };
    let mut resolved = JsdocOptions::default();
    if let Some(value) = config.capitalize_descriptions {
        resolved.capitalize_descriptions = value;
    }
    if let Some(value) = config.description_with_dot {
        resolved.description_with_dot = value;
    }
    if let Some(value) = config.add_default_to_description {
        resolved.add_default_to_description = value;
    }
    if let Some(value) = config.prefer_code_fences {
        resolved.prefer_code_fences = value;
    }
    if let Some(value) = &config.line_wrapping_style {
        resolved.line_wrapping_style = match value.as_str() {
            "greedy" => LineWrappingStyle::Greedy,
            "balance" => LineWrappingStyle::Balance,
            _ => {
                return Err(FormatOptionError::named(
                    "jsdoc lineWrappingStyle",
                    value,
                    "Expected \"greedy\" or \"balance\".",
                ));
            }
        };
    }
    if let Some(value) = &config.comment_line_strategy {
        resolved.comment_line_strategy = match value.as_str() {
            "singleLine" => CommentLineStrategy::SingleLine,
            "multiline" => CommentLineStrategy::Multiline,
            "keep" => CommentLineStrategy::Keep,
            _ => {
                return Err(FormatOptionError::named(
                    "jsdoc commentLineStrategy",
                    value,
                    "Expected \"singleLine\", \"multiline\", or \"keep\".",
                ));
            }
        };
    }
    if let Some(value) = config.separate_tag_groups {
        resolved.separate_tag_groups = value;
    }
    if let Some(value) = config.separate_returns_from_param {
        resolved.separate_returns_from_param = value;
    }
    if let Some(value) = config.bracket_spacing {
        resolved.bracket_spacing = value;
    }
    if let Some(value) = config.description_tag {
        resolved.description_tag = value;
    }
    if let Some(value) = config.keep_unparsable_example_indent {
        resolved.keep_unparsable_example_indent = value;
    }
    Ok(Some(resolved))
}

#[cfg(test)]
mod tests {
    use super::{DynamicTagContract, FormatError, FormatRequest, SourceKind, format};

    fn format_dynamic(expression: &str) -> Result<String, FormatError> {
        let source = format!("const value = <_t0_D0 _t0_A0_={{{expression}}} _t0_Z0_={{null}} />;");
        let original_offsets = [0];
        format(&FormatRequest {
            parse_source: &source,
            source_kind: SourceKind::TypeScriptReact,
            dynamic_tags: Some(DynamicTagContract {
                prefix: "_t0_",
                count: 1,
                original_offsets: &original_offsets,
            }),
            options: None,
        })
        .map(|result| result.code)
    }

    #[test]
    fn dynamic_tag_validator_matches_authoritative_allowed_ast_shapes() {
        for expression in [
            "tag",
            "obj.new",
            "obj?.[key]",
            "(obj)[key]",
            "obj![key]",
            "-1",
            "() => Tag",
            "x = Tag",
            "x += Tag",
            "x++",
            "++x",
            "`d\\${kind}`",
        ] {
            assert!(format_dynamic(expression).is_ok(), "{expression}");
        }
    }

    #[test]
    fn dynamic_tag_validator_rejects_authoritative_disallowed_ast_shapes() {
        for expression in [
            "/x/",
            "null as any",
            "undefined as any",
            "true as any",
            "tag()",
            "condition ? tagName() : Tag",
            "new TagName()",
            "({ tag }).tag",
            "[Tag][0]",
            "'hello' + 'bye'",
            "`d${kind}`",
            "tag`div`",
            "fn!()",
            "fn<string>()",
            "key in [Tag]",
        ] {
            let error = format_dynamic(expression).unwrap_err().to_string();
            assert!(error.contains("dynamic tag"), "{expression}: {error}");
            assert!(error.contains("source byte 0"), "{expression}: {error}");
        }
    }
}
