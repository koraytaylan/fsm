pub(crate) const SPEC_ROWS: &[(&str, &str, &str)] = &[
    (
        "def/shape",
        r#"{"format":"fsm.machine/1"}"#,
        r#"{"format":"fsm.machine/1","name":"okshape","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/unknown_key",
        r#"{"format":"fsm.machine/1","name":"uk","bogus":1,"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"uk2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/cross_region",
        r#"{"format":"fsm.machine/1","name":"xr","regions":[{"name":"left","states":[{"name":"a"}],"initial":"a"},{"name":"right","states":[{"name":"b"}],"initial":"b"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
        r#"{"format":"fsm.machine/1","name":"xr2","regions":[{"name":"left","states":[{"name":"a"}],"initial":"a"},{"name":"right","states":[{"name":"b"}],"initial":"b"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"a"}]}"#,
    ),
    (
        "def/deadline_type",
        r#"{"format":"fsm.machine/1","name":"dt","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"1","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"dt2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"dur(1, s)","to":"a"}]}"#,
    ),
    (
        "def/duplicate_deadline",
        r#"{"format":"fsm.machine/1","name":"dd","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"dur(1, s)","to":"a"},{"name":"later","from":"a","after":"dur(2, s)","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"dd2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"dur(1, s)","to":"a"}]}"#,
    ),
    (
        "def/dup_name",
        r#"{"format":"fsm.machine/1","name":"dn","states":[{"name":"a"},{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"dn2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/reserved_ident",
        r#"{"format":"fsm.machine/1","name":"ri","states":[{"name":"$x"}],"initial":"$x","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"ri2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/eventless_evt",
        r#"{"format":"fsm.machine/1","name":"eevt","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","if":"evt.x > 0","to":"b"}]}"#,
        r#"{"format":"fsm.machine/1","name":"eevt2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.x > 0","to":"b"}]}"#,
    ),
    (
        "def/final_not_leaf",
        r#"{"format":"fsm.machine/1","name":"fnl","states":[{"name":"p","initial":"w","states":[{"name":"w"},{"name":"q","final":true,"initial":"r","states":[{"name":"r"}]}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"fnl2","states":[{"name":"p","initial":"w","states":[{"name":"w"},{"name":"q","initial":"r","states":[{"name":"r"}]}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/final_at_root",
        r#"{"format":"fsm.machine/1","name":"far","states":[{"name":"a"},{"name":"f","final":true}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"far2","states":[{"name":"a"},{"name":"f","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/final_and_terminal",
        r#"{"format":"fsm.machine/1","name":"fat","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true,"terminal":true}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"fat2","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/final_has_transitions",
        r#"{"format":"fsm.machine/1","name":"fht","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true}]}],"initial":"p","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"f","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"fht2","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true}]}],"initial":"p","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"f"}]}"#,
    ),
    (
        "def/final_is_initial",
        r#"{"format":"fsm.machine/1","name":"fii","states":[{"name":"p","initial":"f","states":[{"name":"f","final":true},{"name":"a"}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"fii2","states":[{"name":"p","initial":"a","states":[{"name":"f","final":true},{"name":"a"}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/limit_raises",
        r#"{"format":"fsm.machine/1","name":"lr","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"tick","fields":[],"internal":true},{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","raise":[{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"lr2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"tick","fields":[],"internal":true},{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","raise":[{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"}]}]}"#,
    ),
    (
        "def/eventless_cycle",
        r#"{"format":"fsm.machine/1","name":"ecyc","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"b","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"ecyc2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","to":"b"}]}"#,
    ),
    (
        "def/eventless_from_terminal",
        r#"{"format":"fsm.machine/1","name":"eft","states":[{"name":"a"},{"name":"t","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[{"from":"t","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"eft2","states":[{"name":"a"},{"name":"t"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","to":"t"}]}"#,
    ),
    (
        "def/unknown_state",
        r#"{"format":"fsm.machine/1","name":"us","states":[{"name":"a"}],"initial":"missing","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"us2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/unknown_event",
        r#"{"format":"fsm.machine/1","name":"ue","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","on":"nope"}]}"#,
        r#"{"format":"fsm.machine/1","name":"ue2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e"}]}"#,
    ),
    (
        "def/unknown_effect",
        r#"{"format":"fsm.machine/1","name":"ufx","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"nope","args":{}}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"ufx2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{}}]}]}"#,
    ),
    (
        "def/unknown_enum",
        r#"{"format":"fsm.machine/1","name":"uen","states":[{"name":"a"}],"initial":"a","context":[{"name":"c","ty":{"enum":"Color"},"init":"red"}],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"uen2","enums":{"Color":["red"]},"states":[{"name":"a"}],"initial":"a","context":[{"name":"c","ty":{"enum":"Color"},"init":"red"}],"events":[],"transitions":[]}"#,
    ),
    (
        "def/one_initial",
        r#"{"format":"fsm.machine/1","name":"oi","states":[{"name":"c","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"oi2","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/initial_not_child",
        r#"{"format":"fsm.machine/1","name":"inc","states":[{"name":"c","initial":"z","states":[{"name":"l"}]},{"name":"z"}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"inc2","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/initial_terminal",
        r#"{"format":"fsm.machine/1","name":"it","states":[{"name":"a","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"it2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/initial_is_history",
        r#"{"format":"fsm.machine/1","name":"ih","states":[{"name":"c","initial":"h","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"ih2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/terminal_not_leaf",
        r#"{"format":"fsm.machine/1","name":"tnl","states":[{"name":"c","terminal":true,"initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"tnl2","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/terminal_has_transitions",
        r#"{"format":"fsm.machine/1","name":"tht","states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"b","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"tht2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
    ),
    (
        "def/from_history",
        r#"{"format":"fsm.machine/1","name":"fh","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"h","on":"e"}]}"#,
        r#"{"format":"fsm.machine/1","name":"fh2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e"}]}"#,
    ),
    (
        "def/history_target_from_inside",
        r#"{"format":"fsm.machine/1","name":"hti","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"h"}]}"#,
        r#"{"format":"fsm.machine/1","name":"hti2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"r"}]}"#,
    ),
    (
        "def/multiple_history",
        r#"{"format":"fsm.machine/1","name":"mh","states":[{"name":"c","initial":"l","states":[{"name":"h1","history":"deep"},{"name":"h2","history":"shallow"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"mh2","states":[{"name":"c","initial":"l","states":[{"name":"h1","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/dup_set",
        r#"{"format":"fsm.machine/1","name":"ds","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"},{"target":"n","value":"2"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"ds2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}]}]}"#,
    ),
    (
        "def/assign_type",
        r#"{"format":"fsm.machine/1","name":"at","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"true"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"at2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}]}]}"#,
    ),
    (
        "expr/unknown_var",
        r#"{"format":"fsm.machine/1","name":"uv","states":[{"name":"a"}],"initial":"a","context":[{"name":"flag","ty":"bool","init":"true"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.falg"}]}"#,
        r#"{"format":"fsm.machine/1","name":"uv2","states":[{"name":"a"}],"initial":"a","context":[{"name":"b","ty":"bool","init":"true"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.b"}]}"#,
    ),
    (
        "expr/unknown_field",
        r#"{"format":"fsm.machine/1","name":"ufld","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"evt.nope"}]}"#,
        r#"{"format":"fsm.machine/1","name":"ufld2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.n > 0"}]}"#,
    ),
    (
        "expr/unknown_builtin",
        r#"{"format":"fsm.machine/1","name":"ub","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"nope(1)"}]}"#,
        r#"{"format":"fsm.machine/1","name":"ub2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs(1) == 1"}]}"#,
    ),
    (
        "expr/unknown_enum",
        r#"{"format":"fsm.machine/1","name":"uex","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Rsk.low == Risk.low"}]}"#,
        r#"{"format":"fsm.machine/1","name":"uex2","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.low == Risk.low"}]}"#,
    ),
    (
        "expr/unknown_variant",
        r#"{"format":"fsm.machine/1","name":"uvr","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.lo == Risk.low"}]}"#,
        r#"{"format":"fsm.machine/1","name":"uvr2","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.low == Risk.low"}]}"#,
    ),
    (
        "expr/type_mismatch",
        r#"{"format":"fsm.machine/1","name":"tm","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 + true"}]}"#,
        r#"{"format":"fsm.machine/1","name":"tm2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 + 1 == 2"}]}"#,
    ),
    (
        "expr/mixed_class",
        r#"{"format":"fsm.machine/1","name":"mc","states":[{"name":"a"}],"initial":"a","context":[{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.total + 1 == 0.00"}]}"#,
        r#"{"format":"fsm.machine/1","name":"mc2","states":[{"name":"a"}],"initial":"a","context":[{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.total + 1.00 == 0.00"}]}"#,
    ),
    (
        "expr/chained_cmp",
        r#"{"format":"fsm.machine/1","name":"cc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 < 2 < 3"}]}"#,
        r#"{"format":"fsm.machine/1","name":"cc2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 < 2 and 2 < 3"}]}"#,
    ),
    (
        "expr/cmp_unordered",
        r#"{"format":"fsm.machine/1","name":"cu","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"\"a\" > \"b\""}]}"#,
        r#"{"format":"fsm.machine/1","name":"cu2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 > 0"}]}"#,
    ),
    (
        "expr/parse",
        r#"{"format":"fsm.machine/1","name":"ep","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"("}]}"#,
        r#"{"format":"fsm.machine/1","name":"ep2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
    ),
    (
        "expr/lex",
        r#"{"format":"fsm.machine/1","name":"el","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"@"}]}"#,
        r#"{"format":"fsm.machine/1","name":"el2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
    ),
    (
        "expr/arity",
        r#"{"format":"fsm.machine/1","name":"ea","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs()"}]}"#,
        r#"{"format":"fsm.machine/1","name":"ea2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs(1) == 1"}]}"#,
    ),
    (
        "expr/evt_in_block",
        r#"{"format":"fsm.machine/1","name":"eib","states":[{"name":"a","entry":{"do":[{"target":"n","value":"evt.x"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"eib2","states":[{"name":"a","entry":{"do":[{"target":"n","value":"1"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[]}"#,
    ),
    (
        "expr/evt_in_invariant",
        r#"{"format":"fsm.machine/1","name":"eii","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[],"invariants":[{"name":"i","expr":"evt.x == 1","mode":"enforce"}]}"#,
        r#"{"format":"fsm.machine/1","name":"eii2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[],"invariants":[{"name":"i","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
    ),
    (
        "expr/state_out_of_scope",
        r#"{"format":"fsm.machine/1","name":"esos","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"in(a)"}]}"#,
        r#"{"format":"fsm.machine/1","name":"esos2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
    ),
    (
        "expr/unknown_state",
        r#"{"format":"fsm.machine/1","name":"eus","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"i","expr":"in(nope)","mode":"enforce"}]}"#,
        r#"{"format":"fsm.machine/1","name":"eus2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"i","expr":"in(a)","mode":"enforce"}]}"#,
    ),
    (
        "expr/scale_cap",
        r#"{"format":"fsm.machine/1","name":"esc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0000000 * 1.000000 == 1.0000000"}]}"#,
        r#"{"format":"fsm.machine/1","name":"esc2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.00 * 1.00 == 1.00"}]}"#,
    ),
    (
        "expr/scale_narrow",
        r#"{"format":"fsm.machine/1","name":"esn","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(ctx.d, 1) == 0.0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"esn2","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.d == 0.00"}]}"#,
    ),
    (
        "expr/scale_not_literal",
        r#"{"format":"fsm.machine/1","name":"esl","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"2"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(1, ctx.n) == 1.00"}]}"#,
        r#"{"format":"fsm.machine/1","name":"esl2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"2"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(1, 2) == dec(1, 2)"}]}"#,
    ),
    (
        "expr/dec_range",
        r#"{"format":"fsm.machine/1","name":"edr","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0000000000000 == 1.0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"edr2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.00 == 1.00"}]}"#,
    ),
    (
        "expr/mode_invalid",
        r#"{"format":"fsm.machine/1","name":"emi","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"div(1, 1, 0, nope) == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"emi2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"div(1, 1, 0, down) == div(1, 1, 0, down)"}]}"#,
    ),
    (
        "expr/int_range",
        r#"{"format":"fsm.machine/1","name":"eir","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"99999999999999999999 == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"eir2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 == 1"}]}"#,
    ),
];

pub(crate) const ANALYZE_ROWS: &[(&str, &str)] = &[
    (
        "def/shadowed",
        r#"{"format":"fsm.machine/1","name":"sh","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true","to":"b"},{"from":"a","on":"e","if":"false","to":"b"}]}"#,
    ),
    (
        "def/duplicate_guard",
        r#"{"format":"fsm.machine/1","name":"dg","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true","to":"b"},{"from":"a","on":"e","if":"true","to":"b"}]}"#,
    ),
    (
        "def/ancestor_shadowed",
        r#"{"format":"fsm.machine/1","name":"as","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"c","on":"e"},{"from":"l","on":"e"},{"from":"r","on":"e"}]}"#,
    ),
    (
        "def/unreachable_state",
        r#"{"format":"fsm.machine/1","name":"ur","states":[{"name":"a"},{"name":"ghost"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    ),
    (
        "def/eventless_shadowed",
        r#"{"format":"fsm.machine/1","name":"esh","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"a","if":"ctx.x > 0","to":"b"}]}"#,
    ),
    (
        "def/eventless_cycle_guarded",
        r#"{"format":"fsm.machine/1","name":"ecg","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"b","if":"ctx.x > 0","to":"a"}]}"#,
    ),
    (
        "def/eventless_depth",
        r#"{"format":"fsm.machine/1","name":"edepth","states":[{"name":"s0"},{"name":"s1"},{"name":"s2"},{"name":"s3"},{"name":"s4"},{"name":"s5"},{"name":"s6"},{"name":"s7"},{"name":"s8"},{"name":"s9"},{"name":"s10"},{"name":"s11"},{"name":"s12"},{"name":"s13"},{"name":"s14"},{"name":"s15"},{"name":"s16"},{"name":"s17"},{"name":"s18"},{"name":"s19"},{"name":"s20"},{"name":"s21"},{"name":"s22"},{"name":"s23"},{"name":"s24"},{"name":"s25"},{"name":"s26"},{"name":"s27"},{"name":"s28"},{"name":"s29"},{"name":"s30"},{"name":"s31"},{"name":"s32"},{"name":"s33"},{"name":"s34"}],"initial":"s0","context":[],"events":[],"transitions":[{"from":"s0","to":"s1"},{"from":"s1","to":"s2"},{"from":"s2","to":"s3"},{"from":"s3","to":"s4"},{"from":"s4","to":"s5"},{"from":"s5","to":"s6"},{"from":"s6","to":"s7"},{"from":"s7","to":"s8"},{"from":"s8","to":"s9"},{"from":"s9","to":"s10"},{"from":"s10","to":"s11"},{"from":"s11","to":"s12"},{"from":"s12","to":"s13"},{"from":"s13","to":"s14"},{"from":"s14","to":"s15"},{"from":"s15","to":"s16"},{"from":"s16","to":"s17"},{"from":"s17","to":"s18"},{"from":"s18","to":"s19"},{"from":"s19","to":"s20"},{"from":"s20","to":"s21"},{"from":"s21","to":"s22"},{"from":"s22","to":"s23"},{"from":"s23","to":"s24"},{"from":"s24","to":"s25"},{"from":"s25","to":"s26"},{"from":"s26","to":"s27"},{"from":"s27","to":"s28"},{"from":"s28","to":"s29"},{"from":"s29","to":"s30"},{"from":"s30","to":"s31"},{"from":"s31","to":"s32"},{"from":"s32","to":"s33"},{"from":"s33","to":"s34"}]}"#,
    ),
    (
        "def/eventless_internal_noop",
        r#"{"format":"fsm.machine/1","name":"enoop","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0"}]}"#,
    ),
    (
        "def/create_always_fails",
        r#"{"format":"fsm.machine/1","name":"caf","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"x","expr":"1 == 0","mode":"enforce"}]}"#,
    ),
];
