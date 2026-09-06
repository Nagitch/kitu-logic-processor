use super::*;
use serde_json::json;
use std::sync::Arc;

fn host() -> ScriptHost {
    ScriptHost::new(Limits::default()).unwrap()
}
fn run(host: &ScriptHost, source: &str, input: Value) -> Result<Value, Diagnostic> {
    let program = host.compile(source)?;
    host.invoke(&program, "run", &input)
}

#[test]
fn pure_boss_decisions_round_trip_native_json_types() {
    let host = host();
    let script = host
        .compile(
            r#"
        fn boss(input) {
            if input.phase == 0 && input.timerExpired {
                #{action: "telegraph", duration: 0.8}
            } else if input.hp < input.maxHp / 2 {
                #{action: "pursue", duration: 3.0}
            } else { #{action: "wait", duration: 0.0} }
        }
    "#,
        )
        .unwrap();
    assert_eq!(
        host.invoke(
            &script,
            "boss",
            &json!({"phase":0, "timerExpired":true, "hp":100, "maxHp":100})
        )
        .unwrap(),
        json!({"action":"telegraph", "duration":0.8})
    );
    assert_eq!(
        host.invoke(
            &script,
            "boss",
            &json!({"phase":2, "timerExpired":false, "hp":20, "maxHp":100})
        )
        .unwrap(),
        json!({"action":"pursue", "duration":3.0})
    );
    let input = json!({"null":null, "flag":true, "min":i64::MIN, "max":i64::MAX,
        "fraction":1.5, "text":"テスト", "array":[1,2]});
    assert_eq!(
        run(&host, "fn run(input) { input }", input.clone()).unwrap(),
        input
    );
}

#[test]
fn calls_are_detached_repeatable_and_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ScriptHost>();
    assert_send_sync::<CompiledScript>();
    let host = Arc::new(host());
    let program = host
        .compile(
            r#"
        fn run(input) { input.child.count += 1; input.values[0] = 9; input }
    "#,
        )
        .unwrap();
    let input = json!({"child":{"count":1},"values":[0]});
    let expected = json!({"child":{"count":2},"values":[9]});
    for _ in 0..3 {
        assert_eq!(host.invoke(&program, "run", &input).unwrap(), expected);
    }
    assert_eq!(input, json!({"child":{"count":1},"values":[0]}));
    std::thread::scope(|scope| {
        let first = scope.spawn(|| host.invoke(&program.clone(), "run", &input));
        let second = scope.spawn(|| host.invoke(&program.clone(), "run", &input));
        assert_eq!(first.join().unwrap().unwrap(), expected);
        assert_eq!(second.join().unwrap().unwrap(), expected);
    });
}

#[test]
fn top_level_code_is_not_executed_at_compile_and_is_bounded_at_invoke() {
    let host = ScriptHost::new(Limits {
        max_operations: 100,
        ..Limits::default()
    })
    .unwrap();
    let program = host.compile("loop {} fn run(input) { input }").unwrap();
    let failure = host.invoke(&program, "run", &Value::Null).unwrap_err();
    assert_eq!(failure.kind, "executionLimit");
    // A failed invocation does not poison a reusable host or leak its scope.
    assert_eq!(
        run(&host, "fn run(input) { input + 1 }", json!(1)).unwrap(),
        json!(2)
    );
}

#[test]
fn forbidden_capabilities_have_no_callable_fallbacks() {
    let host = host();
    for source in [
        r#"import "some-file" as external; fn run(x) { x }"#,
        r#"fn run(x) { eval("1 + 1") }"#,
        r#"fn run(x) { print("must not print") }"#,
        r#"fn run(x) { debug("must not print") }"#,
        r#"fn run(x) { Fn("eval").call("1 + 1") }"#,
        r#"fn run(x) { let f = |v| v; f.call(x) }"#,
        r#"fn run(x) { timestamp() }"#,
        r#"fn run(x) { now() }"#,
        r#"fn run(x) { random() }"#,
        r#"fn run(x) { rand() }"#,
        r#"fn run(x) { sleep(0.001) }"#,
        r#"fn run(x) { read_file("Cargo.toml") }"#,
        r#"fn run(x) { write_file("not-created", "text") }"#,
        r#"fn run(x) { exit(0) }"#,
    ] {
        assert!(
            run(&host, source, Value::Null).is_err(),
            "capability unexpectedly allowed: {source}"
        );
    }
}

#[test]
fn diagnostics_preserve_position_and_have_bounded_serializable_messages() {
    let host = host();
    let parse = host
        .compile("fn run(input) {\n let bad = ;\n}")
        .unwrap_err();
    assert_eq!(parse.kind, "compile");
    assert_eq!(parse.line, Some(2));
    assert!(parse.column.is_some());
    let failure = run(&host, "fn run(input) {\n throw input;\n}", json!("failure")).unwrap_err();
    assert_eq!(failure.kind, "runtime");
    assert_eq!(failure.line, Some(2));
    let encoded = serde_json::to_value(&failure).unwrap();
    assert_eq!(
        serde_json::from_value::<Diagnostic>(encoded).unwrap(),
        failure
    );
    assert!(failure.to_string().contains("line 2"));
    let long = run(
        &host,
        "fn run(input) { throw input; }",
        json!("界".repeat(1300)),
    )
    .unwrap_err();
    assert!(long.message.len() <= 1024);
    assert!(long.message.is_char_boundary(long.message.len()));
}

#[test]
fn zero_excessive_or_mismatched_limits_are_rejected() {
    for limits in [
        Limits {
            max_operations: 0,
            ..Limits::default()
        },
        Limits {
            max_source_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_json_depth: 33,
            ..Limits::default()
        },
        Limits {
            max_call_levels: 65,
            ..Limits::default()
        },
    ] {
        assert_eq!(ScriptHost::new(limits).unwrap_err().kind, "invalidLimits");
    }
    let host = host();
    let program = host.compile("fn run(x) { x }").unwrap();
    let other = ScriptHost::new(Limits {
        max_operations: 999,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        other
            .invoke(&program, "run", &Value::Null)
            .unwrap_err()
            .kind,
        "policyMismatch"
    );
    for name in ["", "run()", "run;evil", "é", "1run"] {
        assert_eq!(
            host.invoke(&program, name, &Value::Null).unwrap_err().kind,
            "entrypoint"
        );
    }
    assert_eq!(
        host.invoke(&program, "absent", &Value::Null)
            .unwrap_err()
            .kind,
        "runtime"
    );
}

#[test]
fn compiler_limits_bound_source_expressions_and_function_count() {
    let small = ScriptHost::new(Limits {
        max_source_bytes: 8,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        small.compile("fn run(x) { x }").unwrap_err().kind,
        "sourceLimit"
    );
    let shallow = ScriptHost::new(Limits {
        max_expression_depth: 8,
        ..Limits::default()
    })
    .unwrap();
    let nested = format!("fn run(x) {{ {}x{} }}", "(".repeat(32), ")".repeat(32));
    assert_eq!(shallow.compile(&nested).unwrap_err().kind, "compile");
    let limited = ScriptHost::new(Limits {
        max_functions: 1,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        limited
            .compile("fn first(x) { x } fn run(x) { x }")
            .unwrap_err()
            .kind,
        "compile"
    );
}

#[test]
fn loops_recursion_variables_and_data_growth_are_bounded() {
    let host = ScriptHost::new(Limits {
        max_operations: 200,
        max_call_levels: 8,
        max_variables: 4,
        max_string_bytes: 16,
        max_array_len: 2,
        max_map_len: 2,
        ..Limits::default()
    })
    .unwrap();
    for source in [
        "fn run(x) { loop {} }",
        "fn run(x) { run(x) }",
        "fn run(x) { let a=1; let b=2; let c=3; let d=4; let e=5; e }",
        r#"fn run(x) { "123456789" + "123456789" }"#,
        "fn run(x) { [1,2,3] }",
        "fn run(x) { #{a:1,b:2,c:3} }",
        "fn run(x) { let data=(); loop { data=[data]; } }",
    ] {
        let failure = run(&host, source, Value::Null).unwrap_err();
        assert!(
            ["executionLimit", "compile"].contains(&failure.kind.as_str()),
            "{failure}"
        );
    }
}

#[test]
fn input_and_output_bytes_count_json_escaping_exactly() {
    let input = json!("é\n\t");
    let encoded_size = serde_json::to_vec(&input).unwrap().len();
    let exact = ScriptHost::new(Limits {
        max_input_bytes: encoded_size,
        max_output_bytes: encoded_size,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        run(&exact, "fn run(x) { x }", input.clone()).unwrap(),
        input
    );
    let short_input = ScriptHost::new(Limits {
        max_input_bytes: encoded_size - 1,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        run(&short_input, "fn run(x) { x }", input.clone())
            .unwrap_err()
            .kind,
        "inputLimit"
    );
    let short_output = ScriptHost::new(Limits {
        max_output_bytes: encoded_size - 1,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        run(&short_output, "fn run(x) { x }", input)
            .unwrap_err()
            .kind,
        "outputLimit"
    );
}

#[test]
fn json_boundaries_reject_depth_nodes_large_keys_and_unsigned_overflow() {
    let host = ScriptHost::new(Limits {
        max_json_depth: 2,
        max_json_nodes: 4,
        max_string_bytes: 8,
        max_array_len: 3,
        max_map_len: 2,
        ..Limits::default()
    })
    .unwrap();
    for input in [
        json!([[[[1]]]]),
        json!([[1], [2]]),
        json!({"123456789":1}),
        json!("123456789"),
        json!([1, 2, 3, 4]),
        json!({"a":1,"b":2,"c":3}),
    ] {
        assert_eq!(
            run(&host, "fn run(x) { x }", input).unwrap_err().kind,
            "inputLimit"
        );
    }
    let host = ScriptHost::new(Limits::default()).unwrap();
    assert_eq!(
        run(&host, "fn run(x) { x }", json!(u64::MAX))
            .unwrap_err()
            .kind,
        "input"
    );
    let shallow = ScriptHost::new(Limits {
        max_json_depth: 2,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        run(&shallow, "fn run(x) { [[[[1]]]] }", Value::Null)
            .unwrap_err()
            .kind,
        "outputLimit"
    );
}

#[test]
fn non_json_and_non_finite_outputs_are_never_silently_coerced() {
    let host = host();
    for source in [
        "fn run(x) { 'x' }",
        "fn run(x) { 1..2 }",
        "fn run(x) { 1.0 / 0.0 }",
    ] {
        let failure = run(&host, source, Value::Null).unwrap_err();
        assert!(
            ["output", "runtime"].contains(&failure.kind.as_str()),
            "{failure}"
        );
    }
}
