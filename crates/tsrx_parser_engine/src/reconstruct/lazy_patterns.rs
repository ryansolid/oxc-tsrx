//! Lazy destructuring patterns, projected without `&` and restored with `lazy: true`.

use tsrx_syntax::{OverlayView, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    projection::{map_endpoint, project_authored_start},
    tape_index::ParentIndex,
};

use super::{
    access::{field_value, has_type, list_field, scalar_u32},
    edits::append_node_head,
    objects::find_unique_start,
    spans::{AuthoredStart, record_authored_span},
};

pub(super) fn reconstruct_lazy_patterns(
    tape: &mut FlatTape,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    patterns: &[(u32, RecordIndex)],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    for lazy in overlay.parser_lazy_patterns {
        let pattern = find_pattern(tape, lazy.pattern_start, segments, patterns)?;
        let declarator = declarator_for_pattern(tape, pattern, parents)?;
        if lazy.standalone && pattern_contains_assignment_default(tape, pattern, parents)? {
            return Err(TsrxParseError::AuthoredGrammar(
                "standalone lazy assignment defaults are not supported by the JavaScript TSRX parser"
                    .into(),
            ));
        }

        if tape.field_index(pattern, "lazy").is_some() {
            return Err(TsrxParseError::Unsupported("projected lazy pattern already has metadata"));
        }
        let lazy_value = tape.push_scalar("true")?;
        tape.append_field(pattern, "lazy", lazy_value)?;
        let pattern_end = scalar_u32(tape, pattern, "end")?;
        let authored_end = map_endpoint(segments, pattern_end, false)
            .ok_or(TsrxParseError::Unsupported("lazy pattern end is unmapped"))?;
        record_authored_span(
            starts,
            pattern,
            tsrx_syntax::ByteSpan::new(lazy.pattern_start, authored_end),
        );
        if let Some(declarator) = declarator {
            if lazy.standalone {
                reconstruct_standalone_assignment(
                    tape,
                    pattern,
                    declarator,
                    lazy.ampersand,
                    segments,
                    parents,
                    starts,
                )?;
            } else {
                let declarator_end = scalar_u32(tape, declarator, "end")?;
                let declarator_end = map_endpoint(segments, declarator_end, false)
                    .ok_or(TsrxParseError::Unsupported("lazy declarator end is unmapped"))?;
                record_authored_span(
                    starts,
                    declarator,
                    tsrx_syntax::ByteSpan::new(lazy.ampersand, declarator_end),
                );
            }
        } else if lazy.standalone {
            return Err(TsrxParseError::Unsupported(
                "standalone lazy pattern is not a variable-shaped projection",
            ));
        }
    }
    Ok(())
}

fn pattern_contains_assignment_default(
    tape: &FlatTape,
    pattern: RecordIndex,
    parents: &ParentIndex,
) -> Result<bool, TsrxParseError> {
    for raw in 0..tape.object_count() {
        let object = RecordIndex::new(
            u32::try_from(raw)
                .map_err(|_| TsrxParseError::Unsupported("object index exceeds 4 GiB"))?,
        );
        if !has_type(tape, object, r#""AssignmentPattern""#) {
            continue;
        }
        let mut child = ValueRef::object(object);
        while let Some(parent) = parents.parent_container(child) {
            if parent.as_object() == Some(pattern) {
                return Ok(true);
            }
            child = parent;
        }
    }
    Ok(false)
}

fn reconstruct_standalone_assignment(
    tape: &mut FlatTape,
    pattern: RecordIndex,
    declarator: RecordIndex,
    authored_start: u32,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let declarations =
        parents.parent_container(ValueRef::object(declarator)).and_then(ValueRef::as_list).ok_or(
            TsrxParseError::Unsupported("standalone lazy declarator has no declarations list"),
        )?;
    let declaration = parents
        .parent_container(ValueRef::list(declarations))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported(
            "standalone lazy declarator has no declaration parent",
        ))?;
    if !has_type(tape, declaration, r#""VariableDeclaration""#)
        || list_field(tape, declaration, "declarations")? != declarations
    {
        return Err(TsrxParseError::Unsupported(
            "standalone lazy pattern projection is not a variable declaration",
        ));
    }
    let mut values = tape.values(declarations);
    if values.next().and_then(ValueRef::as_object) != Some(declarator) || values.next().is_some() {
        return Err(TsrxParseError::Unsupported(
            "standalone lazy assignment projection has multiple declarators",
        ));
    }

    let right = field_value(tape, declarator, "init")?;
    let expression_end = map_endpoint(segments, scalar_u32(tape, declarator, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("standalone lazy assignment end is unmapped"))?;
    let statement_end = map_endpoint(segments, scalar_u32(tape, declaration, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("standalone lazy statement end is unmapped"))?;
    let expression_span = tsrx_syntax::ByteSpan::new(authored_start, expression_end);
    let statement_span = tsrx_syntax::ByteSpan::new(authored_start, statement_end);

    tape.clear_fields(declarator)?;
    append_node_head(tape, declarator, r#""AssignmentExpression""#, expression_span)?;
    let operator = tape.push_scalar(r#""=""#)?;
    tape.append_field(declarator, "operator", operator)?;
    tape.append_field(declarator, "left", ValueRef::object(pattern))?;
    tape.append_field(declarator, "right", right)?;
    record_authored_span(starts, declarator, expression_span);

    tape.clear_fields(declaration)?;
    append_node_head(tape, declaration, r#""ExpressionStatement""#, statement_span)?;
    tape.append_field(declaration, "expression", ValueRef::object(declarator))?;
    record_authored_span(starts, declaration, statement_span);
    Ok(())
}

fn find_pattern(
    tape: &FlatTape,
    authored_start: u32,
    segments: &[ProjectionSegment],
    patterns: &[(u32, RecordIndex)],
) -> Result<RecordIndex, TsrxParseError> {
    let projected_start = project_authored_start(segments, authored_start)
        .ok_or(TsrxParseError::Unsupported("lazy pattern start is unmapped"))?;
    let pattern =
        find_unique_start(patterns, projected_start, "lazy pattern is missing or duplicated")?;
    if map_endpoint(segments, scalar_u32(tape, pattern, "start")?, true) != Some(authored_start) {
        return Err(TsrxParseError::Unsupported("lazy pattern start is displaced"));
    }
    Ok(pattern)
}

fn declarator_for_pattern(
    tape: &FlatTape,
    pattern: RecordIndex,
    parents: &ParentIndex,
) -> Result<Option<RecordIndex>, TsrxParseError> {
    let mut child = ValueRef::object(pattern);
    loop {
        let parent = parents
            .parent_container(child)
            .ok_or(TsrxParseError::Unsupported("lazy pattern has no binding owner"))?;
        if let Some(owner) = parent.as_object() {
            let owner_type = super::access::object_type(tape, owner);
            if owner_type == Some(r#""VariableDeclarator""#)
                && field_value(tape, owner, "id")? == child
            {
                return Ok(Some(owner));
            }
            if owner_type == Some(r#""CatchClause""#) && field_value(tape, owner, "param")? == child
            {
                return Ok(None);
            }
            let binding_child = match owner_type {
                Some(r#""AssignmentPattern""#) => field_value(tape, owner, "left")?,
                Some(r#""RestElement""#) => field_value(tape, owner, "argument")?,
                Some(r#""Property""#) => field_value(tape, owner, "value")?,
                _ => {
                    return Err(TsrxParseError::Unsupported(
                        "lazy pattern has an unsupported binding parent",
                    ));
                }
            };
            if binding_child != child {
                return Err(TsrxParseError::Unsupported(
                    "lazy pattern is outside the binding side of its parent",
                ));
            }
            child = parent;
            continue;
        }

        let list = parent
            .as_list()
            .ok_or(TsrxParseError::Unsupported("lazy pattern parent is not a binding list"))?;
        let owner = parents
            .parent_container(ValueRef::list(list))
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("lazy pattern list has no owner"))?;
        match super::access::object_type(tape, owner) {
            Some(
                r#""FunctionDeclaration""#
                | r#""FunctionExpression""#
                | r#""ArrowFunctionExpression""#,
            ) if list_field(tape, owner, "params")? == list => return Ok(None),
            Some(r#""CatchClause""#) => return Ok(None),
            Some(r#""ObjectPattern""#) if list_field(tape, owner, "properties")? == list => {
                child = ValueRef::object(owner);
            }
            Some(r#""ArrayPattern""#) if list_field(tape, owner, "elements")? == list => {
                child = ValueRef::object(owner);
            }
            _ => {
                return Err(TsrxParseError::Unsupported(
                    "lazy pattern list is not part of a binding pattern",
                ));
            }
        }
    }
}
