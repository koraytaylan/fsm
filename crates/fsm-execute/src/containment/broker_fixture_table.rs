//! Checked original handler material shared by independent native fixture axes.

use super::*;

pub(super) fn handler_table(timeout: bool, mcp: bool) -> Value {
    let mut table = parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"on_ok":{"event":"docs_ok"},"on_failed":{"event":"note_added","payload":{"text":"original"}},"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#, &JsonLimits::DEFAULT).unwrap();
    if timeout {
        let Value::Obj(document) = &mut table else {
            unreachable!()
        };
        let Value::Arr(handlers) = document.get_mut("handlers").unwrap() else {
            unreachable!()
        };
        let Value::Obj(handler) = &mut handlers[0] else {
            unreachable!()
        };
        handler.insert(
            "argv".into(),
            Value::Arr(vec![
                Value::Str("/bin/sleep".into()),
                Value::Str("300".into()),
            ]),
        );
    }
    if mcp {
        assert!(!timeout, "fresh MCP success is a distinct fixture axis");
        let Value::Obj(document) = &mut table else {
            unreachable!()
        };
        let Value::Arr(handlers) = document.get_mut("handlers").unwrap() else {
            unreachable!()
        };
        let Value::Obj(handler) = &mut handlers[0] else {
            unreachable!()
        };
        handler.insert("kind".into(), Value::Str("mcp".into()));
        handler.insert("tool".into(), Value::Str("notify".into()));
        handler.insert(
            "arguments".into(),
            object([("fixture", Value::Str("fresh-runner".into()))]),
        );
        handler.insert(
            "argv".into(),
            Value::Arr(vec![
                Value::Str("/usr/bin/python3".into()),
                Value::Str("-u".into()),
                Value::Str("-c".into()),
                Value::Str(MCP_SERVER.into()),
            ]),
        );
    }
    table
}

const MCP_SERVER: &str = r#"import json,sys
calls=0
def reply(request,result):
    print(json.dumps(dict(jsonrpc='2.0',id=request['id'],result=result),separators=(',',':')),flush=True)
for line in sys.stdin:
    request=json.loads(line)
    if request['method']=='initialize':
        reply(request,dict())
    elif request['method']=='tools/call':
        assert request['params']['name']=='notify'
        assert request['params']['arguments']==dict(fixture='fresh-runner')
        calls+=1
        assert calls==1
        reply(request,dict(content=[dict(type='text',text='fresh-runner')],structuredContent=dict(fixture='fresh-runner',calls=calls),isError=False))
"#;
