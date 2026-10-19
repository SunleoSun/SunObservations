use super::*;
use crate::{start_scope, ScopeConfig};

#[test]
fn nested_scopes_restore_parent_and_then_empty_context() {
    assert_eq!(Context::capture().scope_name(), None);
    let outer = start_scope(ScopeConfig::new("outer"));
    assert_eq!(Context::capture().scope_name(), Some("outer"));
    {
        let _inner = start_scope(ScopeConfig::new("inner"));
        assert_eq!(Context::capture().scope_name(), Some("inner"));
    }
    assert_eq!(Context::capture().scope_name(), Some("outer"));
    drop(outer);
    assert_eq!(Context::capture().scope_name(), None);
}

#[test]
fn empty_context_masks_scope_and_unwind_restores_it() {
    let _scope = start_scope(ScopeConfig::new("parent"));
    let result = std::panic::catch_unwind(|| {
        Context::default().run(|| {
            assert_eq!(Context::capture().scope_name(), None);
            panic!("unwind");
        })
    });
    assert!(result.is_err());
    assert_eq!(Context::capture().scope_name(), Some("parent"));
}

#[test]
fn out_of_order_drop_does_not_resurrect_old_scope() {
    let outer = start_scope(ScopeConfig::new("outer"));
    let inner = start_scope(ScopeConfig::new("inner"));
    drop(outer);
    assert_eq!(Context::capture().scope_name(), Some("inner"));
    drop(inner);
    assert_eq!(Context::capture().scope_name(), None);
}

#[test]
fn snapshot_can_outlive_original_guard() {
    let scope = start_scope(ScopeConfig::new("captured"));
    let context = Context::capture();
    drop(scope);
    context.run(|| assert_eq!(Context::capture().scope_name(), Some("captured")));
    assert_eq!(Context::capture().scope_name(), None);
}
