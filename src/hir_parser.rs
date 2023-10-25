use crate::hir_tokenizer::{tokenizer, Span, Spanned, Token};
use chumsky::{prelude::Simple, Parser};
use chumsky::{prelude::*, Stream};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncArg {
    pub type_: Spanned<String>,
    pub name: Spanned<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncSignature {
    pub modifiers: Vec<Spanned<String>>,
    pub ret_type: Spanned<String>,
    pub name: Spanned<String>,
    pub args: Vec<FuncArg>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    GlobalVar {
        name: Spanned<String>,
        def: Vec<Spanned<Token>>,
    },
    FuncDecl {
        signature: FuncSignature,
        addr: Option<Spanned<String>>,
    },
    FuncDef {
        signature: FuncSignature,
        body: FuncBody,
    },
    DbgAnnotation {
        name: Spanned<String>,
        def: Vec<Spanned<Token>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncBody {
    pub opening_bracket: Span,
    pub closing_bracket: Span,
    pub basic_blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicBlock {
    label: Option<Spanned<String>>,
    instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    assignment_target: Option<Spanned<String>>,
    instruction: Spanned<String>,
    jump_targets: Vec<Spanned<Token>>,
}

pub fn parser() -> impl Parser<Token, Vec<Statement>, Error = Simple<Token>> + Clone {
    let eol = just(Token::Newline).or(end().to(Token::Newline));
    let func_modifier = filter_map(|span, token| match token {
        Token::FuncModifier(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected function modifier")),
    });
    let global_name = filter_map(|span, token| match token {
        Token::GlobalName(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected global name")),
    });
    let local_name = filter_map(|span, token: Token| match token {
        Token::LocalName(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected local name")),
    });
    let type_ = filter_map(|span, token: Token| match token {
        Token::Type(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected type name")),
    });
    let ident = filter_map(|span, token| match token {
        Token::Ident(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected global name")),
    });
    let dbg_ref = filter_map(|span, token| match token {
        Token::DebugRef(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected dbgref")),
    });
    let hexnum = filter_map(|span, token| match token {
        Token::HexNum(str) => Ok((str, span)),
        _ => Err(Simple::custom(span, "expected a hexadecimal number")),
    });

    // In some places, we don't really parse the detailed contents but just accept a "token soup"
    let token_soup = none_of(Token::Newline)
        .map_with_span(|tok, span| (tok, span))
        .repeated();

    // Global variables: `@name = <soup>`
    let global_var = global_name
        .then_ignore(just(Token::Punctuation('=')))
        .then(token_soup.clone())
        .then_ignore(eol.clone())
        .map(|(n, d)| Statement::GlobalVar { name: n, def: d });

    // Function arguments
    let func_arg = type_
        .then(local_name)
        .map(|(type_, name)| FuncArg { type_, name });

    // Function arguments
    let func_args = func_arg.separated_by(just(Token::Punctuation(',')));

    // Function signature
    let func_signature = func_modifier
        .repeated()
        .then(type_)
        .then(global_name)
        .then(func_args.delimited_by(just(Token::Punctuation('(')), just(Token::Punctuation(')'))))
        .map(|(((modifiers, ret_type), name), args)| FuncSignature {
            modifiers,
            ret_type,
            name,
            args,
        })
        .boxed();

    // A function address
    let func_addr = just(Token::Punctuation('=')).ignore_then(hexnum);

    // Function declaration
    let func_decl = just(Token::Declare)
        .ignore_then(func_signature.clone())
        .then(func_addr.or_not())
        .then_ignore(eol.clone())
        .map(|(signature, addr)| Statement::FuncDecl { signature, addr });

    // A single instruction
    let instruction = type_
        .ignore_then(local_name)
        .then_ignore(just(Token::Punctuation('=')))
        .or_not()
        .then(ident)
        .then_ignore(none_of(Token::Punctuation(':')).rewind())
        .then_ignore(none_of(Token::Newline).repeated())
        .map(|(target, instruction)| Instruction {
            assignment_target: target,
            instruction,
            jump_targets: vec![],
        });

    // A basic block
    let basic_block = ident
        .then_ignore(just(Token::Punctuation(':')))
        .then_ignore(just(Token::Newline).repeated().at_least(1))
        .then(
            instruction
                .clone()
                .padded_by(just(Token::Newline).repeated())
                .repeated()
                .collect::<Vec<_>>(),
        )
        .map(|(label, instructions)| BasicBlock {
            label: Some(label),
            instructions,
        });

    // Function body
    let func_body = just(Token::Punctuation('{'))
        .map_with_span(|_, span| span)
        .then_ignore(just(Token::Newline).repeated().at_least(1))
        .then(
            instruction
                .padded_by(just(Token::Newline).repeated())
                .repeated()
                .collect::<Vec<_>>()
                .map(|instructions| BasicBlock {
                    label: None,
                    instructions,
                }),
        )
        .then(basic_block.repeated().collect::<Vec<_>>())
        .then(just(Token::Punctuation('}')).map_with_span(|_, span| span))
        .map(
            |(((opening_bracket, initial_bb), mut bbs), closing_bracket)| {
                let mut basic_blocks = vec![initial_bb];
                basic_blocks.append(&mut bbs);
                FuncBody {
                    opening_bracket,
                    closing_bracket,
                    basic_blocks,
                }
            },
        );

    // Function declaration
    let func_def = just(Token::Define)
        .ignore_then(func_signature)
        .then(func_body)
        .then_ignore(eol.clone())
        .map(|(signature, body)| Statement::FuncDef { signature, body });

    // Debug annotation
    let dbg_annotation = dbg_ref
        .then(token_soup)
        .then_ignore(eol.clone())
        .map(|(n, def)| Statement::DbgAnnotation { name: n, def });

    global_var
        .or(func_decl)
        .or(func_def)
        .or(dbg_annotation)
        .padded_by(just(Token::Newline).repeated())
        .recover_with(skip_then_retry_until([]))
        .repeated()
        .then_ignore(end())
}

pub struct ParserResult {
    pub tokens: Vec<Spanned<Token>>,
    pub stmts: Vec<Statement>,
    pub errors: Vec<Simple<String>>,
}

pub fn parse_from_str(src: &str) -> ParserResult {
    // Tokenize
    let (tokens, tok_errs) = tokenizer().parse_recovery(src);

    // Parse
    let (stmts, parse_errs) = if let Some(tokens) = tokens.as_ref() {
        let strlen = src.len();
        parser().parse_recovery(Stream::from_iter(
            strlen..strlen + 1,
            // TODO: can we somehow avoid this copy?
            tokens.clone().into_iter().filter(|t| t.0 != Token::Comment),
        ))
    } else {
        (None, Vec::new())
    };

    // Collect errors from both tokenizer and parser
    let errors = tok_errs
        .into_iter()
        .map(|e| e.map(|c| c.to_string()))
        .chain(parse_errs.into_iter().map(|e| e.map(|tok| tok.to_string())))
        .collect::<Vec<_>>();

    ParserResult {
        tokens: tokens.unwrap_or(Vec::new()),
        stmts: stmts.unwrap_or(Vec::new()),
        errors,
    }
}

#[test]
fn test_parse_globals() {
    let res = parse_from_str("@var1 = [0,0,4,0]");
    assert_eq!(res.errors, []);
    match res.stmts[..] {
        [Statement::GlobalVar { ref name, def: _ }] => {
            assert_eq!(name.0, "@var1");
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    }
}

#[test]
fn test_parse_funcdecl() {
    // Test without arguments and without an address, but with modifiers
    let res = parse_from_str("declare exported int64 @_2_test()");
    assert_eq!(res.errors, []);
    match res.stmts[..] {
        [Statement::FuncDecl {
            signature:
                FuncSignature {
                    ref modifiers,
                    ref ret_type,
                    ref name,
                    ref args,
                },
            addr: None,
        }] => {
            assert_eq!(modifiers.len(), 1);
            assert_eq!(modifiers[0].0, "exported");
            assert_eq!(ret_type.0, "int64");
            assert_eq!(name.0, "@_2_test");
            assert_eq!(args.len(), 0);
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    };

    // Test with arguments and with an address, but without modifiers
    let res = parse_from_str("declare void @foo::bar(int1 %, data128 %baz) = 0x123");
    assert_eq!(res.errors, []);
    match res.stmts[..] {
        [Statement::FuncDecl {
            signature:
                FuncSignature {
                    ref modifiers,
                    ref ret_type,
                    ref name,
                    ref args,
                },
            addr: Some(ref addr),
        }] => {
            assert_eq!(modifiers.len(), 0);
            assert_eq!(ret_type.0, "void");
            assert_eq!(name.0, "@foo::bar");
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].type_.0, "int1");
            assert_eq!(args[0].name.0, "%");
            assert_eq!(args[1].type_.0, "data128");
            assert_eq!(args[1].name.0, "%baz");
            assert_eq!(addr.0, "123");
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    };
}

#[test]
fn test_parse_funcdef() {
    // Test with arguments and with modifiers
    let res = parse_from_str(
        "
    define void @foo::bar(ptr %arg1_2, data128 %baz) {
        some instruction
    body_0:
        int32 %v1 = load int32 ptr %arg1_2 !161  # generateBinaryOperatorFcf
        # comment on separate line
        br int1 %v1 doneIsNull_1, elseIsNull_2
    doneIsNull_1:
        # empty block; not actually valid but accepted
    elseIsNull_2:
        ret
    }",
    );
    assert_eq!(res.errors, []);
    match &res.stmts[..] {
        [Statement::FuncDef {
            signature:
                FuncSignature {
                    modifiers,
                    ret_type,
                    name,
                    args,
                },
            body:
                FuncBody {
                    opening_bracket,
                    closing_bracket,
                    basic_blocks,
                },
        }] => {
            assert_eq!(modifiers.len(), 0);
            assert_eq!(ret_type.0, "void");
            assert_eq!(name.0, "@foo::bar");
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].type_.0, "ptr");
            assert_eq!(args[0].name.0, "%arg1_2");
            assert_eq!(args[1].type_.0, "data128");
            assert_eq!(args[1].name.0, "%baz");

            assert_eq!(*opening_bracket, Span { start: 54, end: 55 });
            assert_eq!(
                *closing_bracket,
                Span {
                    start: 359,
                    end: 360,
                }
            );
            assert_eq!(basic_blocks.len(), 4);
            assert_eq!(basic_blocks[0].label, None);
            assert_eq!(basic_blocks[1].label.as_ref().unwrap().0, "body_0");
            assert_eq!(basic_blocks[2].label.as_ref().unwrap().0, "doneIsNull_1");
            assert_eq!(basic_blocks[3].label.as_ref().unwrap().0, "elseIsNull_2");
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    };
}

#[test]
fn test_parse_dbgannotation() {
    let res = parse_from_str("!123 {}");
    assert_eq!(res.errors, []);
    match res.stmts[..] {
        [Statement::DbgAnnotation { ref name, def: _ }] => {
            assert_eq!(name.0, "123");
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    }
}

#[test]
fn test_recovers_multistmt() {
    let res = parse_from_str(
        "\
    @1 = [12]
    declare void @foo()",
    );
    assert_eq!(res.errors, []);
    match &res.stmts[..] {
        [Statement::GlobalVar {
            name: varname,
            def: _,
        }, Statement::FuncDecl {
            signature:
                FuncSignature {
                    modifiers,
                    ret_type,
                    name: funcname,
                    args,
                },
            addr: None,
        }] => {
            assert_eq!(varname.0, "@1");
            assert_eq!(funcname.0, "@foo");
            assert_eq!(ret_type.0, "void");
            assert_eq!(modifiers.len(), 0);
            assert_eq!(args.len(), 0);
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    }
}

#[test]
fn test_recovers_from_bad_line() {
    let res = parse_from_str(
        "\
    @1 = [12]
    broken line
    @bar = [32]
    ",
    );
    assert_eq!(res.errors.len(), 1);
    match &res.stmts[..] {
        [Statement::GlobalVar {
            name: name1,
            def: _,
        }, Statement::GlobalVar {
            name: name2,
            def: _,
        }] => {
            assert_eq!(name1.0, "@1");
            assert_eq!(name2.0, "@bar");
        }
        _ => panic!("Unexpected parse {:?}", res.stmts),
    }
}

#[test]
fn parses_fcf_example() {
    let res = parse_from_str(&std::fs::read_to_string("examples/fcf.hir").unwrap());
    assert_eq!(res.errors, []);
}

#[test]
fn parses_query_example() {
    let res = parse_from_str(&std::fs::read_to_string("examples/query.hir").unwrap());
    assert_eq!(res.errors, []);
}
