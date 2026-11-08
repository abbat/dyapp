//! Map lint spans to current-source function identities without compiling the application.
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{quote, ToTokens};
use serde_json::{json, Value};
use std::{env, fs, process};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Default)]
struct Functions {
    scope: Vec<String>,
    entries: Vec<Value>,
    active: Vec<usize>,
    depth: usize,
}

fn cfg(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
        .map(|a| a.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Functions {
    fn record(&mut self, kind: &str, span: Span) {
        self.active.push(self.entries.len());
        self.entries.push(json!({
            "id": self.scope.join("::"),
            "kind": kind,
            "start": [span.start().line, span.start().column],
            "end": [span.end().line, span.end().column],
            "nesting": self.depth,
        }));
    }

    fn leave_function(&mut self) {
        self.active.pop().expect("balanced visitor");
        self.scope.pop().expect("balanced scope");
    }
}

impl<'ast> Visit<'ast> for Functions {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.scope
            .push(format!("mod {} {}", node.ident, cfg(&node.attrs)));
        self.depth += usize::from(node.content.is_some());
        visit::visit_item_mod(self, node);
        self.depth -= usize::from(node.content.is_some());
        self.scope.pop();
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let generics = &node.generics;
        let bounds = &generics.where_clause;
        let ty = &node.self_ty;
        let trait_path = node.trait_.as_ref().map(|(_, path, _)| quote!(#path for));
        self.scope.push(format!(
            "impl {} {}",
            quote!(#generics #trait_path #ty #bounds),
            cfg(&node.attrs)
        ));
        self.depth += 1;
        visit::visit_item_impl(self, node);
        self.depth -= 1;
        self.scope.pop();
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.scope
            .push(format!("trait {} {}", node.ident, cfg(&node.attrs)));
        self.depth += 1;
        visit::visit_item_trait(self, node);
        self.depth -= 1;
        self.scope.pop();
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.scope
            .push(format!("fn {} {}", node.sig.ident, cfg(&node.attrs)));
        self.record("function", node.span());
        visit::visit_item_fn(self, node);
        self.leave_function();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.scope
            .push(format!("fn {} {}", node.sig.ident, cfg(&node.attrs)));
        self.record("function", node.span());
        visit::visit_impl_item_fn(self, node);
        self.leave_function();
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if node.default.is_some() {
            self.scope
                .push(format!("fn {} {}", node.sig.ident, cfg(&node.attrs)));
            self.record("function", node.span());
            visit::visit_trait_item_fn(self, node);
            self.leave_function();
        }
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        // Anonymous closures cannot borrow a named function's cognitive-complexity budget.
        self.scope.push("<anonymous closure>".into());
        self.record("closure", node.span());
        visit::visit_expr_closure(self, node);
        self.leave_function();
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.depth += 1;
        for &index in &self.active {
            let old = self.entries[index]["nesting"]
                .as_u64()
                .expect("numeric depth");
            self.entries[index]["nesting"] = json!(old.max(self.depth as u64));
        }
        visit::visit_block(self, node);
        self.depth -= 1;
    }
}

fn functions(source: &str) -> syn::Result<Vec<Value>> {
    let tree = syn::parse_file(source)?;
    let mut functions = Functions::default();
    functions.visit_file(&tree);
    Ok(functions.entries)
}

fn ident_is(token: &TokenTree, expected: &str) -> bool {
    matches!(token, TokenTree::Ident(name)
        if name.to_string().trim_start_matches("r#") == expected)
}

fn threshold_override(tokens: TokenStream) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();
    let assignment = tokens.windows(5).any(|part| {
        ident_is(&part[0], "clippy")
            && matches!(&part[1], TokenTree::Punct(p) if p.as_char() == ':')
            && matches!(&part[2], TokenTree::Punct(p) if p.as_char() == ':')
            && ident_is(&part[3], "cognitive_complexity")
            && matches!(&part[4], TokenTree::Punct(p) if p.as_char() == '=')
    });
    assignment
        || tokens.into_iter().any(|token| match token {
            TokenTree::Group(group) => threshold_override(group.stream()),
            _ => false,
        })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("function-map: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut paths: Vec<_> = env::args().skip(1).collect();
    let check_thresholds = paths.first().is_some_and(|arg| arg == "--check-thresholds");
    if check_thresholds {
        paths.remove(0);
    }
    if paths.is_empty() {
        return Err("expected at least one Rust source path".into());
    }
    for path in paths {
        let source = fs::read_to_string(&path)?;
        if check_thresholds {
            let tokens = source.parse().map_err(|error| format!("{path}: {error}"))?;
            if threshold_override(tokens) {
                return Err(format!("{path}: per-item cognitive threshold override").into());
            }
        } else {
            let entries = functions(&source).map_err(|error| format!("{path}: {error}"))?;
            println!("{}", json!({"path":path,"functions":entries}));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_survive_line_movement_and_distinguish_impls_and_cfg() {
        let source = r#"
struct A; struct B;
impl A { fn run() {} }
impl B { fn run() {} }
mod inner { fn run() {} }
#[cfg(unix)] fn run() {}
#[cfg(windows)] fn run() {}
"#;
        let original = functions(source).unwrap();
        let moved = functions(&format!("// moved\n\n{source}")).unwrap();
        let ids: Vec<_> = original.iter().map(|f| f["id"].clone()).collect();
        assert_eq!(
            ids,
            moved.iter().map(|f| f["id"].clone()).collect::<Vec<_>>()
        );
        let unique: std::collections::HashSet<_> = ids.iter().map(Value::to_string).collect();
        assert_eq!(unique.len(), 5);
        assert_ne!(original[0]["start"], moved[0]["start"]);
    }

    #[test]
    fn nesting_counts_inline_modules_impls_and_closure_blocks() {
        let entries =
            functions("mod m { struct S; impl S { fn f() { let c = || { if true {} }; } } }")
                .unwrap();
        assert_eq!(entries[0]["nesting"], 5);
        assert_eq!(entries[1]["kind"], "closure");
        assert_eq!(entries[1]["nesting"], 5);
    }

    #[test]
    fn threshold_guard_sees_cfg_attributes_but_ignores_literals_and_comments() {
        for source in [
            "#[clippy::cognitive_complexity = 100] fn f() {}",
            "#[r#clippy::r#cognitive_complexity = 100] fn f() {}",
            "#[cfg_attr(test, clippy::cognitive_complexity = 100)] fn f() {}",
        ] {
            assert!(threshold_override(source.parse().unwrap()));
        }
        for source in [
            "// clippy::cognitive_complexity = 100\nfn f() {}",
            r#"const TEXT: &str = "clippy::cognitive_complexity = 100";"#,
            "#[expect(clippy::cognitive_complexity)] fn f() {}",
        ] {
            assert!(!threshold_override(source.parse().unwrap()));
        }
    }

    #[test]
    fn malformed_source_fails_instead_of_returning_an_empty_inventory() {
        assert!(functions("fn broken(").is_err());
    }
}
