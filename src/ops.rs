//! `error` ops — the error-raising primitive (ADR-0092 gap (a)).
//!
//! `error.raise(v)` aborts the current evaluation with `v` as the error
//! payload: it never returns a value. The [`OpRegistry`] maps the op name
//! declared in `handl-packages/error/error.hnl` to the Rust handl below.
//!
//! This is the raise half of the ADR-0092 error story. There is still no
//! error-*capture* primitive (`assert.throws`/`test.run` need a
//! handl-reachable `EvalContext::try_call_value`), so a raised error
//! always propagates out of `eval` as a normal eval failure.
//!
//! Payload convention: a `String` payload is the message verbatim; any
//! other value is embedded as its JSON serialization. Errors surface as
//! [`OpError::Other`], matching how `collections`' `collection is
//! empty` and other contract violations report today.

use std::sync::Arc;

use handl_ops::OpRegistry;
use handl_ops::{Op, OpContext, OpError};
use handl_value::{sym_seeded, tagged_to_value};
use serde_json::{Map, Value};

/// The op names declared in `error.hnl`.
pub const ERROR_OPS: &[&str] = &["error.raise", "error.try"];

/// The Rust handl for `op "error.raise" : Value -> a`.
///
/// Fails loudly: always returns `Err`, never `Ok`. A missing payload
/// defaults to a generic message so `error.raise()` still aborts.
#[derive(Debug, Clone, Copy)]
pub struct Raise;

impl Op for Raise {
    fn call(&self, args: &[Value], _ctx: &mut dyn OpContext) -> Result<Value, OpError> {
        let message = match args.first() {
            None => "error raised".to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(v) => serde_json::to_string(v).unwrap_or_else(|_| v.to_string()),
        };
        Err(OpError::Other(message))
    }
}

/// `error.try(thunk: Fn) -> result`
///
/// Calls the provided arrow with `null` for each of its parameters and
/// captures any raised `OpError`. Returns the ADR-0152 `__ctor__`
/// encoding of `result.ok(v)` / `result.err(e)` — identical to what
/// `lib-result`'s `ok`/`err` constructors mint in `.hnl`:
/// `{"__ctor__": <sym "data.Result.ok"|"data.Result.err">, "value": v,
/// "type": {"__former__": <sym "data.Result">, "indices": [null, null]}}`.
/// `match` dispatches on the ctor sym's display name (`ok`/`err`), so
/// consumers need no `result` import.
#[derive(Debug, Clone, Copy)]
pub struct Try;

fn eval_error_message(e: OpError) -> String {
    match e {
        OpError::Other(s)
        | OpError::TypeMismatch(s)
        | OpError::UnknownFunction(s)
        | OpError::UnknownVariable(s)
        | OpError::ResourceLimit(s) => s,
        OpError::Yield => "evaluation yielded".to_string(),
    }
}

impl Op for Try {
    fn call(&self, args: &[Value], ctx: &mut dyn OpContext) -> Result<Value, OpError> {
        let f = args.first().ok_or_else(|| {
            OpError::TypeMismatch(format!("expected {} args, got {}", 1, args.len()))
        })?;

        // Thunks are typically nullary, but callers may write `_ ~> ...`
        // or `x ~> ...`; supply `null` for every declared parameter so the
        // body runs regardless of whether the parameter is used.
        let params_len = f
            .as_object()
            .and_then(|o| o.get("params"))
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let call_args: Vec<Value> = (0..params_len).map(|_| Value::Null).collect();

        match ctx.try_call_value(f, &call_args) {
            Some(Ok(v)) => Ok(result_ctor("data.Result.ok", v)),
            Some(Err(e)) => Ok(result_ctor(
                "data.Result.err",
                Value::String(eval_error_message(e)),
            )),
            None => Err(OpError::Other(
                "error.try: no value callable in context".to_string(),
            )),
        }
    }
}

/// Mint the ADR-0152 `__ctor__` encoding of a `Result` value — the same
/// record `lib-result`'s `ok(v)`/`err(e)` produce in `.hnl` via
/// `sym.seeded`: the `__ctor__` sym is `Sym::seeded(ctor_seed)` (handle =
/// content hash of the seed, name = last `.`-segment), `value` carries the
/// payload, and `type` is the `data.Result` former applied to the two
/// `Result<T, E>` parameters (`indices: [null, null]`).
fn result_ctor(ctor_seed: &str, payload: Value) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "__ctor__".to_string(),
        tagged_to_value(&sym_seeded(ctor_seed)),
    );
    obj.insert("value".to_string(), payload);
    let mut ty = Map::new();
    ty.insert(
        "__former__".to_string(),
        tagged_to_value(&sym_seeded("data.Result")),
    );
    ty.insert(
        "indices".to_string(),
        Value::Array(vec![Value::Null, Value::Null]),
    );
    obj.insert("type".to_string(), Value::Object(ty));
    Value::Object(obj)
}

/// Build the [`OpRegistry`] for the `error` module.
pub fn error_op_registry() -> OpRegistry {
    let mut registry = OpRegistry::new();
    registry.register("error.raise", Arc::new(Raise));
    registry.register("error.try", Arc::new(Try));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal [`OpContext`] stub: `try_call_value` replays a canned
    /// outcome; `eval_expr` is unused by `error.try`.
    struct StubCtx {
        outcome: Option<Result<Value, OpError>>,
    }

    impl OpContext for StubCtx {
        fn eval_expr(&mut self, _expr: &str, _vars: &[(String, Value)]) -> Result<Value, OpError> {
            Err(OpError::Other("eval_expr unused in tests".to_string()))
        }

        fn try_call_value(
            &mut self,
            _callee: &Value,
            _args: &[Value],
        ) -> Option<Result<Value, OpError>> {
            self.outcome.clone()
        }
    }

    fn sym_name(v: &Value) -> &str {
        v["__sym__"]["name"].as_str().expect("sym encoding")
    }

    #[test]
    fn result_ctor_mints_ad_r0152_shape() {
        let v = result_ctor("data.Result.ok", Value::from(42));
        let obj = v.as_object().expect("ctor object");
        assert!(!obj.contains_key("__tag__"));
        assert_eq!(sym_name(&v["__ctor__"]), "ok");
        assert_eq!(v["value"], Value::from(42));
        assert_eq!(sym_name(&v["type"]["__former__"]), "Result");
        assert_eq!(
            v["type"]["indices"],
            Value::Array(vec![Value::Null, Value::Null])
        );
        // The seeded handle is the content hash — stable and shared with
        // the `.hnl` `sym.seeded` mint of the same seed.
        let direct = tagged_to_value(&sym_seeded("data.Result.ok"));
        assert_eq!(v["__ctor__"], direct);
    }

    #[test]
    fn try_success_mints_ok_ctor() {
        let mut ctx = StubCtx {
            outcome: Some(Ok(Value::from("fine"))),
        };
        let out = Try.call(&[Value::Null], &mut ctx).expect("try ok");
        assert_eq!(sym_name(&out["__ctor__"]), "ok");
        assert_eq!(out["value"], Value::from("fine"));
    }

    #[test]
    fn try_failure_mints_err_ctor_with_message() {
        let mut ctx = StubCtx {
            outcome: Some(Err(OpError::Other("boom".to_string()))),
        };
        let out = Try.call(&[Value::Null], &mut ctx).expect("try err");
        assert_eq!(sym_name(&out["__ctor__"]), "err");
        assert_eq!(out["value"], Value::from("boom"));
    }

    #[test]
    fn try_requires_one_arg() {
        let mut ctx = StubCtx { outcome: None };
        assert!(Try.call(&[], &mut ctx).is_err());
    }

    #[test]
    fn try_uncallable_value_is_error() {
        let mut ctx = StubCtx { outcome: None };
        assert!(Try.call(&[Value::Null], &mut ctx).is_err());
    }
}
