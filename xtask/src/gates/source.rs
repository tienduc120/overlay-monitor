//! Rules that read the syntax tree: what a dependency edge cannot say.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned as _;
use syn::visit::{self, Visit};
use syn::{AttrStyle, Attribute, Expr, Fields, ImplItem, Lit, Meta, Type};

use super::Finding;
use super::workspace::{Crate, Location};

/// The doc-comment marker that makes a struct an aggregate root (ADR-0007).
const AGGREGATE_MARKER: &str = "agent:aggregate";

/// The one crate that may expect `unsafe_code` (ADR-0012).
const UNSAFE_CRATE: &str = "platform-win32";

/// `files` are (path relative to the repository, source) pairs of one crate.
pub(crate) fn check(c: &Crate, files: &[(String, String)]) -> Vec<Finding> {
    let check_restore =
        matches!(c.location(), Location::Layer { layer, .. } if layer != "domain" && layer != "infrastructure");
    let mut v = Visitor {
        no_std: c.no_std,
        may_use_unsafe: c.name == UNSAFE_CRATE,
        check_restore,
        ..Visitor::default()
    };
    for (path, src) in files {
        v.file.clone_from(path);
        v.test_file = path.ends_with("/tests.rs") || path.contains("/tests/");
        match syn::parse_file(src) {
            Ok(file) => v.visit_file(&file),
            Err(e) => v.push(e.span().start().line, "source-parse", format!("does not parse: {e}")),
        }
    }
    v.finish()
}

struct Aggregate {
    name: String,
    at: String,
    id_type: Option<String>,
}

#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent per-crate and per-file switches, not a state machine"
)]
struct Visitor {
    no_std: bool,
    may_use_unsafe: bool,
    check_restore: bool,
    file: String,
    test_file: bool,
    found: Vec<Finding>,
    aggregates: Vec<Aggregate>,
    local_types: BTreeSet<String>,
    inherent_fns: BTreeMap<String, BTreeSet<String>>,
}

impl Visitor {
    fn push(&mut self, line: usize, rule: &'static str, message: String) {
        self.found.push(Finding {
            path: format!("{}:{line}", self.file),
            rule,
            message,
        });
    }

    fn finish(mut self) -> Vec<Finding> {
        for agg in &self.aggregates {
            let name = &agg.name;
            if !agg.id_type.as_ref().is_some_and(|t| self.local_types.contains(t)) {
                let message = format!(
                    "aggregate `{name}` needs a field `id` whose type this crate defines (struct {name}Id(..)), \
                     so one aggregate's id cannot be passed for another's"
                );
                self.found.push(Finding {
                    path: agg.at.clone(),
                    rule: "source-aggregate",
                    message,
                });
            }
            let fns = self.inherent_fns.get(name);
            let missing: Vec<&str> = ["new", "restore"]
                .into_iter()
                .filter(|f| !fns.is_some_and(|fns| fns.contains(*f)))
                .collect();
            if !missing.is_empty() {
                let message = format!(
                    "aggregate `{name}` needs `{name}::new` (checks the invariants) and `{name}::restore` (rebuilds a \
                     stored value; only infrastructure calls it); missing: {}",
                    missing.join(", ")
                );
                self.found.push(Finding {
                    path: agg.at.clone(),
                    rule: "source-aggregate",
                    message,
                });
            }
        }
        self.found
    }
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_attribute(&mut self, i: &'ast Attribute) {
        if !self.may_use_unsafe && names_unsafe_code(i) {
            let message = "unsafe code lives only in platform-win32 (ADR-0012); wrap the call there behind a safe \
                           function"
                .to_owned();
            self.push(i.span().start().line, "source-unsafe-scope", message);
        }
        if matches!(i.style, AttrStyle::Inner(_)) && i.path().is_ident("allow") {
            let message = "a crate- or module-wide #![allow] hides every finding below it; put \
                           #[expect(lint, reason = \"...\")] on the one item that needs it"
                .to_owned();
            self.push(i.span().start().line, "source-inner-allow", message);
        }
    }

    fn visit_item_extern_crate(&mut self, i: &'ast syn::ItemExternCrate) {
        if self.no_std && i.ident == "std" && !is_cfg_test(&i.attrs) {
            let message = "this crate is #![no_std] so the compiler keeps I/O, threads, and the clock out; \
                           `extern crate std` brings them back (allowed only under #[cfg(test)])"
                .to_owned();
            self.push(i.span().start().line, "source-extern-std", message);
        }
        visit::visit_item_extern_crate(self, i);
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_cfg_test(&i.attrs) {
            return visit::visit_item_mod(self, i);
        }
        if i.content.is_some() {
            let message = format!(
                "move the tests to a sibling file: `#[cfg(test)] mod {};` and `<module>/{}.rs`",
                i.ident, i.ident
            );
            self.push(i.span().start().line, "source-inline-tests", message);
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.local_types.insert(i.ident.to_string());
        if i.attrs
            .iter()
            .any(|a| doc_text(a).is_some_and(|d| d.contains(AGGREGATE_MARKER)))
        {
            let id_type = match &i.fields {
                Fields::Named(named) => named
                    .named
                    .iter()
                    .find(|f| f.ident.as_ref().is_some_and(|name| name == "id"))
                    .and_then(|f| last_ident(&f.ty)),
                Fields::Unnamed(_) | Fields::Unit => None,
            };
            let at = format!("{}:{}", self.file, i.span().start().line);
            self.aggregates.push(Aggregate {
                name: i.ident.to_string(),
                at,
                id_type,
            });
        }
        visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.local_types.insert(i.ident.to_string());
        visit::visit_item_enum(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_none()
            && let Some(name) = last_ident(&i.self_ty)
        {
            self.inherent_fns
                .entry(name)
                .or_default()
                .extend(i.items.iter().filter_map(fn_name));
        }
        visit::visit_item_impl(self, i);
    }

    /// Any path to `X::restore`, called or passed as a value (`.map(Note::restore)`). A call written inside a macro's
    /// arguments is not parsed by syn and is not seen.
    fn visit_expr_path(&mut self, i: &'ast syn::ExprPath) {
        if self.check_restore
            && !self.test_file
            && i.path.segments.len() >= 2
            && i.path.segments.last().is_some_and(|s| s.ident == "restore")
        {
            let message = "only infrastructure rebuilds an aggregate with `restore`, which skips the invariant \
                           checks; build new values with `new`"
                .to_owned();
            self.push(i.span().start().line, "source-restore", message);
        }
        visit::visit_expr_path(self, i);
    }

    /// A `#[cfg(test)]` function is test code wherever it sits.
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if !is_cfg_test(&i.attrs) {
            visit::visit_item_fn(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if !is_cfg_test(&i.attrs) {
            visit::visit_impl_item_fn(self, i);
        }
    }
}

/// An `expect` or `allow`, inner or outer, whose list names the `unsafe_code` lint.
fn names_unsafe_code(a: &Attribute) -> bool {
    (a.path().is_ident("expect") || a.path().is_ident("allow"))
        && a.meta.require_list().is_ok_and(|l| {
            l.tokens
                .to_string()
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == "unsafe_code")
        })
}

fn fn_name(item: &ImplItem) -> Option<String> {
    if let ImplItem::Fn(f) = item {
        Some(f.sig.ident.to_string())
    } else {
        None
    }
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") && a.meta.require_list().is_ok_and(|l| l.tokens.to_string() == "test"))
}

fn doc_text(a: &Attribute) -> Option<String> {
    if let Meta::NameValue(nv) = &a.meta
        && nv.path.is_ident("doc")
        && let Expr::Lit(lit) = &nv.value
        && let Lit::Str(s) = &lit.lit
    {
        return Some(s.value());
    }
    None
}

fn last_ident(ty: &Type) -> Option<String> {
    if let Type::Path(p) = ty {
        p.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
