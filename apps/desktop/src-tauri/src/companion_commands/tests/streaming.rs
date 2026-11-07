//! Streaming replies, cancel and the three read-only lookups (Sprint 12, S12-T15 and S12-T16).
//! A local mock provider on loopback; no real network.

use std::sync::atomic::AtomicBool;

use super::*;

fn body_n(m: &Mock, n: usize) -> serde_json::Value {
    let reqs = m.requests.lock().expect("log");
    let (_, body) = reqs
        .get(n)
        .expect("request")
        .split_once("\r\n\r\n")
        .expect("body");
    serde_json::from_str(body).expect("json")
}

fn request_count(m: &Mock) -> usize {
    m.requests.lock().expect("log").len()
}

fn enable_tools(core: &Core) {
    set_tools_enabled(core, true).expect("tools on");
}

fn deltas(events: &Mutex<Vec<StreamEvent>>) -> String {
    events
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

// -- streaming ---------------------------------------------------------------------------

#[test]
fn a_streamed_reply_is_cleaned_chunk_by_chunk_stored_and_booked_with_reported_usage() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let hostile = "data: {\"choices\":[{\"delta\":{\"content\":\"safe \\u202etext\\u0007\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" end\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":7}}\n\ndata: [DONE]\n\n";
    let m = mock_sse(vec![hostile.to_owned()]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let events = Mutex::new(Vec::new());
    let reply = send_events(&core, t, &pid, "hi", &Scope::None, &|e| {
        events.lock().expect("events").push(e);
    })
    .expect("send");
    assert_eq!(reply.text, "safe text end");
    assert_eq!(deltas(&events), "safe text end");
    // Every event belongs to the one request, and the UI can tell which.
    let ids: std::collections::HashSet<String> = events
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 1);
    assert!(ids.iter().next().expect("id").starts_with("test-req-"));
    assert_eq!(used_tokens(&core), 47);
    let ms = messages(&core, t).expect("m");
    assert_eq!(ms[0].input_tokens, Some(40));
    assert!(body_n(&m, 0)["stream"] == true);
}

#[test]
fn a_dropped_stream_fails_the_message_keeps_no_partial_reply_and_counts_what_arrived() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let m = mock(vec![Sc::Sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"half a reply that never finishes\"}}]}\n\n",
    )]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let events = Mutex::new(Vec::new());
    let e = send_events(&core, t, &pid, "hi", &Scope::None, &|ev| {
        events.lock().expect("events").push(ev);
    })
    .unwrap_err();
    assert_eq!(kind(&e), "providerError");
    assert_eq!(deltas(&events), "half a reply that never finishes");
    let ms = messages(&core, t).expect("m");
    assert_eq!(
        ms.len(),
        1,
        "no assistant message was written for a partial reply"
    );
    assert_eq!(ms[0].status, "failed");
    assert!(used_tokens(&core) > 0, "the partial reply was not counted");
}

#[test]
fn the_budget_blocks_the_next_send_after_a_partial_reply_was_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let m = mock(vec![Sc::Sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
    )]);
    let pid = profile(&core, &m.base, Some(1));
    let t = thread(&core);
    assert_eq!(
        kind(&send(&core, t, &pid, "hi", &Scope::None).unwrap_err()),
        "providerError"
    );
    assert_eq!(
        kind(&send(&core, t, &pid, "again", &Scope::None).unwrap_err()),
        "budgetReached"
    );
    assert_eq!(request_count(&m), 1, "no second request");
}

#[test]
fn malformed_streams_become_a_clean_invalid_response() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let m = mock(vec![Sc::Sse("data: {broken\n\n")]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let e = send(&core, t, &pid, "hi", &Scope::None).unwrap_err();
    assert_eq!(kind(&e), "invalidResponse");
    assert!(!e.message.contains("broken"));
}

#[test]
fn cancel_mid_stream_frees_the_thread_and_late_chunks_never_reach_history() {
    let dir = tempfile::tempdir().expect("dir");
    let core = Arc::new(core_with(dir.path(), memory(), 20_000));
    let m = mock(vec![
        Sc::SseLate(
            "data: {\"choices\":[{\"delta\":{\"content\":\"early words\"}}]}\n\n",
            900,
            "data: {\"choices\":[{\"delta\":{\"content\":\"LATE-CHUNK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ),
        Sc::Ok,
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let got_first = Arc::new(AtomicBool::new(false));
    let (c2, pid2, flag) = (Arc::clone(&core), pid.clone(), Arc::clone(&got_first));
    let first_events = Arc::new(Mutex::new(Vec::new()));
    let fe = Arc::clone(&first_events);
    let first = std::thread::spawn(move || {
        send_events(&c2, t, &pid2, "stream me", &Scope::None, &move |e| {
            if matches!(e, StreamEvent::Delta { .. }) {
                flag.store(true, Ordering::SeqCst);
            }
            fe.lock().expect("events").push(e);
        })
    });
    let start = Instant::now();
    while !got_first.load(Ordering::SeqCst) {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    cancel(&core, t);
    let cancelled_at = Instant::now();
    let e = first.join().expect("join").unwrap_err();
    assert_eq!(kind(&e), "cancelled");
    assert!(
        cancelled_at.elapsed() < Duration::from_millis(600),
        "returned at once"
    );
    // The thread is free while the old worker is still connected.
    let again = send(&core, t, &pid, "second", &Scope::None).expect("new send in the same thread");
    assert_eq!(again.text, "a reply");
    // Let the old stream finish its late chunk, then look at everything that was stored.
    std::thread::sleep(Duration::from_millis(1400));
    let ms = messages(&core, t).expect("m");
    for m in &ms {
        assert!(!m.text.contains("LATE-CHUNK"), "late data reached history");
    }
    assert!(
        !deltas(&first_events).contains("LATE-CHUNK"),
        "late data reached the UI"
    );
    assert_eq!(ms[0].status, "failed");
    assert_eq!(ms[0].error.as_ref().expect("e").kind, "cancelled");
    assert_eq!(ms.len(), 3);
    // The cancelled call is counted (an estimate with the partial reply) next to the second
    // send's 10 + 5 reported tokens.
    let start = Instant::now();
    while used_tokens(&core) <= 15 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "cancelled usage lost"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

// -- remembered context --------------------------------------------------------------------

#[test]
fn the_last_context_choice_is_remembered_on_send_and_lookups_start_off() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    let p = prefs(&core);
    assert!(!p.tools_enabled, "lookups are off by default");
    assert_eq!(p.last_scope, None);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    // A refused send (stale hash) remembers nothing.
    assert!(raw_send(&core, t, &pid, "hi", &summary(), Some("deadbeef")).is_err());
    assert_eq!(prefs(&core).last_scope, None);
    send(&core, t, &pid, "hi", &summary()).expect("send");
    assert_eq!(prefs(&core).last_scope, Some(summary()));
    send(&core, t, &pid, "plain", &Scope::None).expect("send");
    assert_eq!(prefs(&core).last_scope, Some(Scope::None));
    // It is plain local data: ids only, no text, no key.
    let stored = with_store(&core, |s| s.preference(COMPANION_PREFS_KEY))
        .expect("pref")
        .expect("saved");
    assert!(!stored.contains(NOTES) && !stored.contains(KEY));
}

#[test]
fn unreadable_settings_fall_back_to_defaults() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    with_store(&core, |s| {
        s.set_preference(COMPANION_PREFS_KEY, "{\"toolsEnabled\":\"yes\"}")
    })
    .expect("set");
    assert_eq!(prefs(&core), CompanionPrefs::default());
    assert!(set_tools_enabled(&core, true).expect("on").tools_enabled);
    assert!(prefs(&core).tools_enabled);
    assert!(!set_tools_enabled(&core, false).expect("off").tools_enabled);
}

// -- lookups ---------------------------------------------------------------------------------

#[test]
fn lookups_are_offered_only_when_switched_on() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let m = mock_sse(vec![OA_TEXT.to_owned(), OA_TEXT.to_owned()]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "one", &Scope::None).expect("send");
    let off = body_n(&m, 0);
    assert!(off.get("tools").is_none());
    assert_eq!(off["messages"][0]["content"], ::companion::SYSTEM_PROMPT);
    enable_tools(&core);
    send(&core, t, &pid, "two", &Scope::None).expect("send");
    let on = body_n(&m, 1);
    assert_eq!(on["tools"].as_array().expect("tools").len(), 3);
    assert_eq!(
        on["messages"][0]["content"],
        ::companion::SYSTEM_PROMPT_WITH_TOOLS
    );
    assert!(on.get("tool_choice").is_none());
}

#[test]
fn an_approved_lookup_shows_the_exact_text_and_only_then_sends_it() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_1", "session_overview", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let shown = Mutex::new(String::new());
    let seen_before_answer = Mutex::new(0usize);
    let reply = send_events(
        &core,
        t,
        &pid,
        "what are my agents doing?",
        &Scope::None,
        &|e| {
            if let StreamEvent::ToolRequest {
                request_id,
                approval_id,
                result_text,
                host,
                tool,
                ..
            } = e
            {
                *shown.lock().expect("shown") = result_text;
                // Nothing has gone to the provider besides the first request.
                *seen_before_answer.lock().expect("n") = request_count(&m);
                assert_eq!(tool, "session_overview");
                assert!(host.starts_with("127.0.0.1:"));
                tool_decision(&core, &request_id, &approval_id, true).expect("approve");
            }
        },
    )
    .expect("send");
    assert_eq!(reply.text, "streamed reply");
    assert_eq!(*seen_before_answer.lock().expect("n"), 1);
    let shown = shown.into_inner().expect("shown");
    // The second request carries exactly the text the user saw.
    let second = body_n(&m, 1);
    let msgs = second["messages"].as_array().expect("messages");
    let tool_msg = msgs
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool message");
    assert_eq!(tool_msg["tool_call_id"], "call_1");
    assert_eq!(tool_msg["content"], shown);
    assert!(shown.contains("claude-code") && shown.contains("needs attention"));
    // Task text and ids stay out unless the capture opt-in is on.
    for secret in ["Fix login", "Make login work", NOTES, "s1", "ITEM-ONE"] {
        assert!(!shown.contains(secret), "{secret} leaked into the overview");
    }
    // Both calls count: 30 + 4 and 40 + 7.
    assert_eq!(used_tokens(&core), 81);
    let ms = messages(&core, t).expect("m");
    assert_eq!(
        (ms[0].input_tokens, ms[0].output_tokens),
        (Some(70), Some(11))
    );
    // The lookup is stored with the message, with its host.
    let rec = &ms[0].tool_calls;
    assert_eq!(rec.len(), 1);
    assert_eq!(rec[0].status, "approved");
    assert_eq!(rec[0].result_text, shown);
    assert!(
        rec[0]
            .host
            .as_deref()
            .is_some_and(|h| h.starts_with("127.0.0.1:"))
    );
}

#[test]
fn a_declined_lookup_sends_only_the_fixed_sentence_and_stores_no_data() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_9", "session_overview", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send_events(&core, t, &pid, "agents?", &Scope::None, &|e| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = e
        {
            tool_decision(&core, &request_id, &approval_id, false).expect("decline");
        }
    })
    .expect("send");
    let second = body_n(&m, 1);
    let tool_msg = second["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool message")
        .clone();
    assert_eq!(tool_msg["content"], "The user declined.");
    assert!(!second.to_string().contains("claude-code: needs attention"));
    let rec = messages(&core, t).expect("m")[0].tool_calls.clone();
    assert_eq!(rec[0].status, "declined");
    assert_eq!(rec[0].result_text, "The user declined.");
    assert!(rec[0].card.is_none());
}

#[test]
fn unknown_tools_get_a_fixed_sentence_without_asking_and_the_round_limit_holds() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("c_a", "run_shell", "{\"cmd\":\"rm -rf /\"}"),
        oa_call("c_b", "suggest_break", "{}"),
        oa_call("c_c", "suggest_break", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let asked = Mutex::new(Vec::<String>::new());
    let skipped = Mutex::new(Vec::<String>::new());
    let notices = Mutex::new(Vec::<(String, Option<String>)>::new());
    let reply = send_events(&core, t, &pid, "hi", &Scope::None, &|e| match e {
        StreamEvent::ToolRequest {
            request_id,
            approval_id,
            tool,
            ..
        } => {
            asked.lock().expect("a").push(tool);
            tool_decision(&core, &request_id, &approval_id, true).expect("approve");
        }
        StreamEvent::ToolSkipped { text, .. } => skipped.lock().expect("s").push(text),
        StreamEvent::ToolNotice { reason, tool, .. } => {
            notices.lock().expect("n").push((reason, tool))
        }
        _ => {}
    })
    .expect("send");
    assert_eq!(reply.text, "streamed reply");
    // Owner decision 27.6: an unknown lookup also shows a notice with the safe tool name, and the
    // notice is stored with the message so it reappears in history. No arguments anywhere.
    assert_eq!(
        notices.lock().expect("n").as_slice(),
        [("unknown".to_owned(), Some("run_shell".to_owned()))]
    );
    let stored = messages(&core, t).expect("m")[0].tool_notices.clone();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].reason, "unknown");
    assert_eq!(stored[0].tool.as_deref(), Some("run_shell"));
    assert!(!format!("{stored:?}").contains("rm -rf"));
    // The made-up tool never reached the user as a request and got a fixed sentence.
    assert_eq!(
        skipped.lock().expect("s").as_slice(),
        ["That lookup is not available."]
    );
    assert_eq!(asked.lock().expect("a").len(), 2);
    let content = body_n(&m, 1)["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|m| m["tool_call_id"] == "c_a")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .unwrap_or_default();
    assert_eq!(content, "That lookup is not available.");
    // Three rounds of lookups, then a request without tools: the model has to answer in words.
    assert_eq!(request_count(&m), 4);
    for n in 0..3 {
        assert!(
            body_n(&m, n).get("tools").is_some(),
            "round {n} offers tools"
        );
    }
    assert!(
        body_n(&m, 3).get("tools").is_none(),
        "tools are withdrawn after the limit"
    );
    // M1: and the last request carries no tool blocks either, because without tool definitions
    // a provider rejects them. The earlier rounds are plain text there.
    let last = body_n(&m, 3).to_string();
    for needle in ["tool_call_id", "tool_calls", "\"role\":\"tool\""] {
        assert!(!last.contains(needle), "final request has {needle}");
    }
    assert!(last.contains("That lookup is not available."));
}

#[test]
fn an_old_cards_approval_cannot_answer_a_newer_card() {
    // L3: answers are matched by an id made here for each card, never by the provider's call id.
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("same_id", "suggest_break", "{}"),
        oa_call("same_id", "suggest_break", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let seen = Mutex::new(Vec::<String>::new());
    send_events(&core, t, &pid, "break?", &Scope::None, &|e| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = e
        {
            let mut ids = seen.lock().expect("ids");
            assert_ne!(approval_id, "same_id", "not the provider's id");
            if let Some(old) = ids.first() {
                // The first card was already answered; its id does nothing to this one.
                assert!(tool_decision(&core, &request_id, old, true).is_err());
                assert!(!ids.contains(&approval_id), "a fresh id for every card");
            }
            ids.push(approval_id.clone());
            drop(ids);
            tool_decision(&core, &request_id, &approval_id, true).expect("approve");
        }
    })
    .expect("send");
    assert_eq!(seen.lock().expect("ids").len(), 2);
}

#[test]
fn calls_over_the_per_message_limit_get_a_fixed_sentence_and_are_not_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let five: Vec<(String, &str, &str)> = (0..5)
        .map(|i| (format!("c_{i}"), "suggest_break", "{}"))
        .collect();
    let m = mock_sse(vec![oa_calls(&five), OA_TEXT.to_owned()]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let asked = Mutex::new(0usize);
    let skipped = Mutex::new(Vec::<String>::new());
    let notices = Mutex::new(Vec::<(String, Option<String>)>::new());
    send_events(&core, t, &pid, "hi", &Scope::None, &|e| match e {
        StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } => {
            *asked.lock().expect("a") += 1;
            tool_decision(&core, &request_id, &approval_id, true).expect("approve");
        }
        StreamEvent::ToolSkipped { text, .. } => skipped.lock().expect("s").push(text),
        StreamEvent::ToolNotice { reason, tool, .. } => {
            notices.lock().expect("n").push((reason, tool))
        }
        _ => {}
    })
    .expect("send");
    assert_eq!(
        notices.lock().expect("n").as_slice(),
        [
            ("limit".to_owned(), Some("suggest_break".to_owned())),
            ("limit".to_owned(), Some("suggest_break".to_owned()))
        ]
    );
    let stored = messages(&core, t).expect("m")[0].tool_notices.clone();
    assert_eq!(
        stored.iter().map(|n| n.reason.as_str()).collect::<Vec<_>>(),
        ["limit", "limit"]
    );
    assert_eq!(*asked.lock().expect("a"), 3);
    assert_eq!(
        skipped.lock().expect("s").as_slice(),
        ["The limit of lookups for this message was reached."; 2]
    );
    let second = body_n(&m, 1);
    // The limit was reached, so this request has no tools and the five answers are plain text.
    assert!(second.get("tools").is_none());
    let text = second["messages"].to_string();
    assert!(!text.contains("tool_call_id"));
    assert_eq!(
        text.matches("[suggest_break]").count(),
        5,
        "every call gets an answer"
    );
}

#[test]
fn a_malformed_lookup_shows_a_notice_and_an_unsafe_name_or_arguments_never_reach_it() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_calls(&[
            (
                "c_a".to_owned(),
                "suggest_break",
                "{\"available_minutes\": 9999, \"note\": \"SECRETARGS\"}",
            ),
            ("c_b".to_owned(), "Run Shell!", "{\"x\": \"SECRETARGS\"}"),
        ]),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let events = Mutex::new(Vec::<StreamEvent>::new());
    let asked = Mutex::new(0usize);
    send_events(&core, t, &pid, "hi", &Scope::None, &|e| {
        if matches!(e, StreamEvent::ToolRequest { .. }) {
            *asked.lock().expect("a") += 1;
        }
        events.lock().expect("e").push(e);
    })
    .expect("send");
    assert_eq!(*asked.lock().expect("a"), 0, "nothing to approve");
    let seen: Vec<(String, Option<String>)> = events
        .lock()
        .expect("e")
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolNotice { reason, tool, .. } => Some((reason.clone(), tool.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("malformed".to_owned(), Some("suggest_break".to_owned())),
            ("unknown".to_owned(), None)
        ]
    );
    let stored = messages(&core, t).expect("m")[0].tool_notices.clone();
    assert_eq!(
        stored
            .iter()
            .map(|n| (n.reason.as_str(), n.tool.as_deref()))
            .collect::<Vec<_>>(),
        [("malformed", Some("suggest_break")), ("unknown", None)]
    );
    // The model's arguments and odd name are in no event and not in the stored notices.
    let all = format!("{:?}{stored:?}", events.lock().expect("e"));
    assert!(!all.contains("SECRETARGS"), "{all}");
    assert!(!all.contains("Run Shell"), "{all}");
    // And they are camelCase on the wire.
    let json = serde_json::to_string(&StreamEvent::ToolNotice {
        request_id: "r".into(),
        reason: "limit".into(),
        tool: None,
    })
    .expect("json");
    assert!(json.contains("\"type\":\"toolNotice\"") && json.contains("\"requestId\""));
}

#[test]
fn cancelling_while_a_lookup_waits_sends_nothing_more() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_1", "session_overview", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let e = send_events(&core, t, &pid, "agents?", &Scope::None, &|ev| {
        if matches!(ev, StreamEvent::ToolRequest { .. }) {
            cancel(&core, t);
        }
    })
    .unwrap_err();
    assert_eq!(kind(&e), "cancelled");
    assert_eq!(request_count(&m), 1, "the lookup result was never sent");
    assert_eq!(messages(&core, t).expect("m")[0].status, "failed");
    assert!(messages(&core, t).expect("m")[0].tool_calls.is_empty());
    // The thread is free.
    assert!(core.companion.begin(t, "test-request-id").is_ok());
}

#[test]
fn only_the_waiting_lookup_of_the_right_request_can_be_answered() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_1", "suggest_break", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    assert!(tool_decision(&core, "r-nothing-waiting", "call_1", true).is_err());
    send_events(&core, t, &pid, "break?", &Scope::None, &|e| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = e
        {
            assert!(tool_decision(&core, &request_id, "another_call", true).is_err());
            assert!(tool_decision(&core, "another-request", &approval_id, true).is_err());
            assert!(tool_decision(&core, "bad id!", &approval_id, true).is_err());
            tool_decision(&core, &request_id, &approval_id, true).expect("right one");
            // Answering twice does nothing.
            assert!(tool_decision(&core, &request_id, &approval_id, false).is_err());
        }
    })
    .expect("send");
}

#[test]
fn the_budget_is_checked_again_between_lookup_rounds() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    // The first call costs 30 + 4 = 34 tokens, which is the whole budget.
    let m = mock_sse(vec![
        oa_call("call_1", "suggest_break", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, Some(34));
    let t = thread(&core);
    let e = send_events(&core, t, &pid, "break?", &Scope::None, &|ev| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = ev
        {
            tool_decision(&core, &request_id, &approval_id, true).expect("approve");
        }
    })
    .unwrap_err();
    assert_eq!(kind(&e), "budgetReached");
    assert_eq!(request_count(&m), 1);
    assert_eq!(used_tokens(&core), 34, "the first call stayed on the books");
}

#[test]
fn lookups_belong_to_their_thread_and_go_with_it() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_1", "suggest_break", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send_events(&core, t, &pid, "break?", &Scope::None, &|ev| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = ev
        {
            tool_decision(&core, &request_id, &approval_id, true).expect("approve");
        }
    })
    .expect("send");
    assert_eq!(
        with_store(&core, |s| s.companion_tool_calls(t))
            .expect("calls")
            .len(),
        1
    );
    delete_thread(&core, t).expect("delete");
    let other = thread(&core);
    assert!(
        with_store(&core, |s| s.companion_tool_calls(other))
            .expect("calls")
            .is_empty()
    );
}

// -- the lookups' own data -------------------------------------------------------------------

#[test]
fn the_overview_names_no_task_text_unless_the_capture_opt_in_is_on() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    let now = OffsetDateTime::now_utc();
    let off = lookups::session_overview(&core, now).expect("overview");
    assert!(off.contains("claude-code"));
    assert!(!off.contains("Fix login"));
    with_store(&core, |s| {
        s.set_preference(
            crate::context::CONTEXT_PREFS_KEY,
            "{\"retentionDays\":30,\"capture\":{\"claude-code\":true}}",
        )
    })
    .expect("pref");
    let on = lookups::session_overview(&core, now).expect("overview");
    assert!(
        on.contains("Fix login"),
        "the short stored label is allowed with the opt-in"
    );
    for never in ["Make login work", NOTES, "ITEM-ONE", "s1"] {
        assert!(!on.contains(never), "{never} must never be in the overview");
    }
}

#[test]
fn imported_sessions_are_never_presented_as_reported_states() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let now = OffsetDateTime::now_utc();
    with_store(&core, |s| {
        s.import_session(
            "codex",
            "019db995-c01a-7b2e-9f3c-1234567890ab",
            agent_core::SessionState::NeedsAttention,
            now,
            now,
        )
        .map(|_| ())
    })
    .expect("import");
    let out = lookups::session_overview(&core, now).expect("overview");
    assert!(!out.contains("needs attention for"), "{out}");
    assert!(out.contains("imported from agent logs"));
    let facts = with_store(&core, |s| {
        session_facts(s, "codex", "019db995-c01a-7b2e-9f3c-1234567890ab")
    })
    .expect("facts");
    assert!(facts.status.contains("guessed from the agent's log"));
    assert!(!facts.status.contains("as last reported"));
}

#[test]
fn an_inferred_end_and_a_catch_up_guess_are_never_presented_as_reported() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let now = OffsetDateTime::now_utc();
    let earlier = now - time::Duration::minutes(30);
    let event = |n: u32, session: &str, kind: &str| {
        agent_core::parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{}"}}"#,
            earlier
                .format(&time::format_description::well_known::Rfc3339)
                .expect("fmt")
        ))
        .expect("event")
    };
    let process = local_store::ProcessKey::new(4242, 99);
    with_store(&core, |s| {
        s.ingest_with_process(&event(1, "gone", "working"), earlier, Some(process))?;
        s.end_session_process_gone("claude-code", "gone", process, now, now)?;
        s.ingest(&event(2, "hinted", "working"), earlier)?;
        s.apply_log_hint(
            "claude-code",
            "hinted",
            agent_core::SessionState::Idle,
            now - time::Duration::minutes(5),
            now,
        )
        .map(|_| ())
    })
    .expect("setup");
    let out = lookups::session_overview(&core, now).expect("overview");
    assert!(
        out.contains("ended (inferred: the agent's program is no longer running"),
        "{out}"
    );
    assert!(out.contains("1 more have a state filled in from the agent's log at start-up"));
    assert!(
        !out.contains("imported from agent logs"),
        "a catch-up guess is not an import: {out}"
    );
    let gone = with_store(&core, |s| session_facts(s, "claude-code", "gone")).expect("facts");
    assert!(
        gone.status.contains("inferred by Lullshift"),
        "{}",
        gone.status
    );
    assert!(!gone.status.contains("as last reported"));
    let hinted = with_store(&core, |s| session_facts(s, "claude-code", "hinted")).expect("facts");
    assert!(
        hinted
            .status
            .contains("guessed from the agent's log, filled in at start-up")
    );
}

#[test]
fn break_suggestions_and_help_come_from_installed_activities_only() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    let installed = crate::activities::list_ids(&core).expect("ids");
    assert!(!installed.is_empty());
    let r = lookups::run(
        &core,
        &::companion::tools::Request::SuggestBreak {
            available_minutes: Some(10),
        },
        OffsetDateTime::now_utc(),
    )
    .expect("suggest");
    let card = r.card.expect("a card");
    assert!(installed.contains(&card.activity_id));
    assert!(r.text.contains("nothing has been started"));
    for bad in ["\\", ".git", "http://", "https://"] {
        assert!(!r.text.contains(bad), "{bad} in a suggestion");
    }
    let help = lookups::run(
        &core,
        &::companion::tools::Request::ActivityHelp {
            activity_id: card.activity_id.clone(),
        },
        OffsetDateTime::now_utc(),
    )
    .expect("help");
    assert!(help.text.contains("Help for") || help.text.contains("no help text"));
    assert!(help.card.is_none());
    let none = lookups::run(
        &core,
        &::companion::tools::Request::ActivityHelp {
            activity_id: "not.installed.anywhere".into(),
        },
        OffsetDateTime::now_utc(),
    )
    .expect("help");
    assert_eq!(none.text, "No installed activity has that id.");
}

#[test]
fn no_lookup_text_or_event_carries_a_key() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 5000);
    seed_session(&core);
    enable_tools(&core);
    let m = mock_sse(vec![
        oa_call("call_1", "session_overview", "{}"),
        OA_TEXT.to_owned(),
    ]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let events = Mutex::new(Vec::new());
    send_events(&core, t, &pid, "agents?", &Scope::None, &|e| {
        if let StreamEvent::ToolRequest {
            request_id,
            approval_id,
            ..
        } = &e
        {
            tool_decision(&core, request_id, approval_id, true).expect("approve");
        }
        events.lock().expect("events").push(e);
    })
    .expect("send");
    let all = serde_json::to_string(&*events.lock().expect("events")).expect("json");
    assert!(!all.contains(KEY));
    let stored = serde_json::to_string(&messages(&core, t).expect("m")).unwrap_or_default();
    assert!(!stored.contains(KEY));
}
