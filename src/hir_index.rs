use crate::{
    hir_parser::{BasicBlock, Statement},
    hir_tokenizer::{Span, Spanned, Token},
};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseDefKind {
    Decl,
    Def,
    Use,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct UseDefList {
    pub decls: Vec<Span>,
    pub defs: Vec<Span>,
    pub uses: Vec<Span>,
}

impl UseDefList {
    pub fn get_use_def_kind(&self, ud: UseDefKind) -> &Vec<std::ops::Range<usize>> {
        match ud {
            UseDefKind::Decl => &self.decls,
            UseDefKind::Def => &self.defs,
            UseDefKind::Use => &self.uses,
        }
    }

    pub fn get_use_def_kind_mut(&mut self, ud: UseDefKind) -> &mut Vec<std::ops::Range<usize>> {
        match ud {
            UseDefKind::Decl => &mut self.decls,
            UseDefKind::Def => &mut self.defs,
            UseDefKind::Use => &mut self.uses,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FunctionBody {
    pub name: Spanned<String>,
    pub complete_range: Span,
    pub labels: HashMap<String, UseDefList>,
    pub local_vars: HashMap<String, UseDefList>,
    pub basic_blocks: Vec<BasicBlock>,
    pub incoming_bb_branches: HashMap<String, Vec<Spanned<String>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    GlobalVar,
    Function,
    DbgAnnotation,
    Label,
    LocalVar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub span: Span,
    pub use_def: UseDefKind,
    pub symbol_kind: SymbolKind,
    pub name: String,
    pub func_body_id: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HIRIndex {
    pub global_vars: HashMap<String, UseDefList>,
    pub functions: HashMap<String, UseDefList>,
    pub dgb_annotations: HashMap<String, UseDefList>,
    pub reverse_idx: BTreeMap<usize, SymbolOccurrence>,
    pub function_bodies: Vec<FunctionBody>,
}

impl HIRIndex {
    pub fn get_by_symbol_kind<'a>(
        &'a self,
        k: SymbolKind,
        fb: Option<&'a FunctionBody>,
    ) -> &'a HashMap<String, UseDefList> {
        match k {
            SymbolKind::GlobalVar => &self.global_vars,
            SymbolKind::Function => &self.functions,
            SymbolKind::DbgAnnotation => &self.dgb_annotations,
            SymbolKind::Label => &fb.unwrap().labels,
            SymbolKind::LocalVar => &fb.unwrap().local_vars,
        }
    }

    fn add_internal(
        map: &mut HashMap<String, UseDefList>,
        reverse_idx: &mut BTreeMap<usize, SymbolOccurrence>,
        func_body_id: Option<usize>,
        k: SymbolKind,
        ud: UseDefKind,
        span: &Span,
        name: &str,
    ) {
        let e = map.entry(name.to_string()).or_default();
        if e.decls.contains(span) || e.defs.contains(span) || e.uses.contains(span) {
            return;
        }
        e.get_use_def_kind_mut(ud).push(span.clone());
        reverse_idx.insert(
            span.start,
            SymbolOccurrence {
                span: span.clone(),
                use_def: ud,
                symbol_kind: k,
                name: name.to_string(),
                func_body_id,
            },
        );
    }

    pub fn add_global(&mut self, k: SymbolKind, ud: UseDefKind, span: &Span, name: &str) {
        let map = match k {
            SymbolKind::GlobalVar => &mut self.global_vars,
            SymbolKind::Function => &mut self.functions,
            SymbolKind::DbgAnnotation => &mut self.dgb_annotations,
            SymbolKind::Label => unreachable!(),
            SymbolKind::LocalVar => unreachable!(),
        };
        Self::add_internal(map, &mut self.reverse_idx, None, k, ud, span, name);
    }

    pub fn add_global_spanned(&mut self, k: SymbolKind, ud: UseDefKind, spanned: &Spanned<String>) {
        self.add_global(k, ud, &spanned.1, &spanned.0)
    }

    pub fn add_func_local(
        &mut self,
        func_body_id: usize,
        k: SymbolKind,
        ud: UseDefKind,
        span: &Span,
        name: &str,
    ) {
        let map = match k {
            SymbolKind::GlobalVar => unreachable!(),
            SymbolKind::Function => unreachable!(),
            SymbolKind::DbgAnnotation => unreachable!(),
            SymbolKind::Label => &mut self.function_bodies[func_body_id].labels,
            SymbolKind::LocalVar => &mut self.function_bodies[func_body_id].local_vars,
        };
        Self::add_internal(
            map,
            &mut self.reverse_idx,
            Some(func_body_id),
            k,
            ud,
            span,
            name,
        );
    }

    pub fn add_func_local_spanned(
        &mut self,
        func_body_id: usize,
        k: SymbolKind,
        ud: UseDefKind,
        spanned: &Spanned<String>,
    ) {
        self.add_func_local(func_body_id, k, ud, &spanned.1, &spanned.0)
    }

    pub fn find_symbol_at_position(&self, pos: usize) -> Option<&SymbolOccurrence> {
        self.reverse_idx
            .iter()
            .map(|e| e.1)
            .find(|e| e.span.contains(&pos))
        /* TODO use 'upper_bound'
        self.reverse_idx
            .upper_bound(Bound::Included(&pos))
            .value()
            .filter(|s| s.span.contains(&pos))
        */
    }
}

pub fn create_index(tokens: &[Spanned<Token>], stmts: &[Statement]) -> HIRIndex {
    let mut index = HIRIndex {
        ..Default::default()
    };

    // Index all definitions / declarations based on the actual parse tree
    for s in stmts.iter() {
        match s {
            Statement::GlobalVar { name, def: _ } => {
                index.add_global_spanned(SymbolKind::GlobalVar, UseDefKind::Def, name)
            }
            Statement::FuncDecl { signature, .. } => {
                index.add_global_spanned(SymbolKind::Function, UseDefKind::Decl, &signature.name)
            }
            Statement::FuncDependencies { .. } => {}
            Statement::DbgAnnotation { name, def: _ } => {
                index.add_global_spanned(SymbolKind::DbgAnnotation, UseDefKind::Def, name)
            }
            // FuncDef is a bit more complicated, since we also index the structure of the function body
            // (labels, local variables) here.
            Statement::FuncDef {
                define_kw,
                signature,
                body,
            } => {
                index.add_global_spanned(SymbolKind::Function, UseDefKind::Def, &signature.name);
                index.function_bodies.push(FunctionBody {
                    name: signature.name.clone(),
                    complete_range: Span {
                        start: define_kw.start,
                        end: body.closing_bracket.end,
                    },
                    basic_blocks: body.basic_blocks.clone(),
                    ..Default::default()
                });
                let func_body_id = index.function_bodies.len() - 1;
                // Index the function arguments
                for arg in &signature.args {
                    index.add_func_local_spanned(
                        func_body_id,
                        SymbolKind::LocalVar,
                        UseDefKind::Def,
                        &arg.name,
                    );
                }
                // Index the labels and local variables defined in each basic block
                for bb in &body.basic_blocks {
                    // Index the label
                    if bb.label.is_some() {
                        index.add_func_local_spanned(
                            func_body_id,
                            SymbolKind::Label,
                            UseDefKind::Def,
                            bb.label.as_ref().unwrap(),
                        )
                    }
                    // Index the variables and label references of all instructions in the basic block
                    for i in &bb.instructions {
                        if i.assignment_target.is_some() {
                            index.add_func_local_spanned(
                                func_body_id,
                                SymbolKind::LocalVar,
                                UseDefKind::Def,
                                i.assignment_target.as_ref().unwrap(),
                            )
                        }
                        for bb_ref in &i.basic_block_refs {
                            index.add_func_local_spanned(
                                func_body_id,
                                SymbolKind::Label,
                                UseDefKind::Use,
                                bb_ref,
                            );
                            if i.instruction.0 != "phi" && bb.label.is_some() {
                                let label = bb.label.as_ref().unwrap();
                                let func_body = &mut index.function_bodies[func_body_id];
                                let incoming_list = func_body
                                    .incoming_bb_branches
                                    .entry(bb_ref.0.clone())
                                    .or_default();
                                if !incoming_list.contains(label) {
                                    incoming_list.push(label.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Index all uses based on the raw token stream
    let mut func_body_id: Option<usize> = None;
    for t in tokens.iter() {
        match &t.0 {
            Token::GlobalName(name) => {
                if index.functions.contains_key(&name.to_string()) {
                    index.add_global(SymbolKind::Function, UseDefKind::Use, &t.1, name)
                } else {
                    index.add_global(SymbolKind::GlobalVar, UseDefKind::Use, &t.1, name)
                }
            }
            Token::DebugRef(name) => {
                index.add_global(SymbolKind::DbgAnnotation, UseDefKind::Use, &t.1, name)
            }
            Token::LocalName(name) => {
                if func_body_id.is_none()
                    || !index.function_bodies[func_body_id.unwrap()]
                        .complete_range
                        .contains(&t.1.start)
                {
                    func_body_id = index
                        .function_bodies
                        .iter()
                        .enumerate()
                        .find(|(_, e)| e.complete_range.contains(&t.1.start))
                        .map(|t| t.0);
                }
                if let Some(func_body_id) = func_body_id {
                    index.add_func_local(
                        func_body_id,
                        SymbolKind::LocalVar,
                        UseDefKind::Use,
                        &t.1,
                        name,
                    )
                }
            }
            _ => {}
        }
    }

    index
}

#[test]
fn test_index() {
    // Test with arguments and with modifiers
    let res = crate::hir_parser::parse_from_str(
        "
        @a = \"test\"
        declare int32 @foo::bar(ptr %, int32 %baz) = 0x1234
        declare int32 @_test1(int32 %foo, data128 %baz)
        define void @_test1(int32 %foo, data128 %baz) {
          some instruction
          body_0:
            int32 %res_1 = call @foo::bar(ptr @a, int32 %foo)
            br switch_1
          switch_1:
            switch int32 %res_1, default=done_1, int32 0 label=done_1
          done_1:
            ret int32 %res_1 !21 # some comment
        }
        !21 {\"some\": \"data\"}",
    );
    assert_eq!(res.errors, []);
    let idx = create_index(&res.tokens, &res.stmts);
    assert_eq!(
        idx.global_vars,
        HashMap::from([(
            "@a".to_string(),
            UseDefList {
                decls: Vec::new(),
                defs: vec![9..11],
                uses: vec![284..286]
            }
        )])
    );

    assert_eq!(
        idx.functions,
        HashMap::from([
            (
                "@foo::bar".to_string(),
                UseDefList {
                    decls: vec![43..52],
                    defs: Vec::new(),
                    uses: vec![270..279]
                }
            ),
            (
                "@_test1".to_string(),
                UseDefList {
                    decls: vec![103..110],
                    defs: vec![157..164],
                    uses: Vec::new()
                }
            ),
        ])
    );

    assert_eq!(
        idx.dgb_annotations,
        HashMap::from([(
            "!21".to_string(),
            UseDefList {
                decls: Vec::new(),
                defs: vec![498..501],
                uses: vec![461..464]
            }
        )])
    );

    match &idx.function_bodies[..] {
        [FunctionBody {
            name,
            complete_range,
            labels,
            local_vars,
            basic_blocks,
            incoming_bb_branches,
        }] => {
            assert_eq!(name.0, "@_test1");
            assert_eq!(name.1, 157..164);
            assert_eq!(*complete_range, 145..489);
            assert_eq!(
                *labels,
                HashMap::from([
                    (
                        "body_0".to_string(),
                        UseDefList {
                            decls: Vec::new(),
                            defs: vec![230..237],
                            uses: Vec::new(),
                        }
                    ),
                    (
                        "switch_1".to_string(),
                        UseDefList {
                            decls: vec![],
                            defs: vec![334..343],
                            uses: vec![315..323]
                        }
                    ),
                    (
                        "done_1".to_string(),
                        UseDefList {
                            decls: Vec::new(),
                            defs: vec![424..431],
                            uses: vec![385..391, 407..413]
                        }
                    ),
                ])
            );
            assert_eq!(
                *local_vars,
                HashMap::from([
                    (
                        "%foo".to_string(),
                        UseDefList {
                            decls: Vec::new(),
                            defs: vec![171..175],
                            uses: vec![294..298],
                        }
                    ),
                    (
                        "%baz".to_string(),
                        UseDefList {
                            decls: Vec::new(),
                            defs: vec![185..189],
                            uses: Vec::new(),
                        }
                    ),
                    (
                        "%res_1".to_string(),
                        UseDefList {
                            decls: Vec::new(),
                            defs: vec![256..262],
                            uses: vec![369..375, 454..460],
                        }
                    ),
                ])
            );
            assert_eq!(basic_blocks.len(), 4);
            assert_eq!(
                *incoming_bb_branches,
                HashMap::from([
                    (
                        "switch_1".to_string(),
                        vec![("body_0".to_string(), 230..237)]
                    ),
                    (
                        "done_1".to_string(),
                        // Note that `switch_1` is listed only once, although it mentions `done1`
                        // as its target twice.
                        vec![("switch_1".to_string(), 334..343)]
                    ),
                ])
            );
        }
        _ => panic!("Unexpected index contents {:?}", res.stmts),
    };
}
