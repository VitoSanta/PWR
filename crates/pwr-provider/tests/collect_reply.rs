//! A reply is the whole stream, not its first chunk.

use futures_util::StreamExt as _;
use pwr_domain::{GenerationMetrics, ModelChunk, ToolCall};
use pwr_provider::{ModelStream, ProviderError, collect_reply};

fn stream(chunks: Vec<Result<ModelChunk, ProviderError>>) -> ModelStream {
    Box::pin(futures_util::stream::iter(chunks))
}
fn thinking(text: &str) -> ModelChunk {
    ModelChunk {
        thinking: Some(text.into()),
        ..Default::default()
    }
}
fn content(text: &str) -> ModelChunk {
    ModelChunk {
        content: text.into(),
        ..Default::default()
    }
}

/// The defect this exists to prevent, found three times: in the capability
/// probe, in calibration, and in the action loop.
#[tokio::test]
async fn an_answer_after_leading_thinking_chunks_is_assembled() {
    let reply = collect_reply(stream(vec![
        Ok(thinking("The user")),
        Ok(thinking(" wants")),
        Ok(content(r#"{"capability":"#)),
        Ok(content(r#""list_tree","max_entries":10}"#)),
        Ok(ModelChunk {
            done: true,
            ..Default::default()
        }),
    ]))
    .await
    .unwrap();
    assert_eq!(
        reply.content,
        r#"{"capability":"list_tree","max_entries":10}"#
    );
    // Reasoning stays out of the answer channel; a parser must never see it.
    assert_eq!(reply.thinking, "The user wants");
    assert_eq!(reply.chunks, 5);
}

#[tokio::test]
async fn tool_calls_are_collected_from_wherever_they_arrive() {
    let call = |name: &str| ModelChunk {
        tool_calls: vec![ToolCall {
            name: name.into(),
            arguments: serde_json::json!({}),
            id: None,
        }],
        ..Default::default()
    };
    let reply = collect_reply(stream(vec![
        Ok(thinking("hm")),
        Ok(call("first")),
        Ok(content("text")),
        Ok(call("second")),
        Ok(ModelChunk {
            done: true,
            ..Default::default()
        }),
    ]))
    .await
    .unwrap();
    assert_eq!(
        reply
            .tool_calls
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
}

#[tokio::test]
async fn terminal_metrics_are_kept() {
    let reply = collect_reply(stream(vec![
        Ok(content("x")),
        Ok(ModelChunk {
            metrics: Some(GenerationMetrics {
                generated_tokens: Some(42),
                generation_duration_ns: Some(1_000_000_000),
                ..Default::default()
            }),
            done: true,
            ..Default::default()
        }),
    ]))
    .await
    .unwrap();
    assert_eq!(reply.metrics.unwrap().tokens_per_second(), Some(42.0));
}

#[tokio::test]
async fn the_stream_stops_at_done() {
    let reply = collect_reply(stream(vec![
        Ok(content("kept")),
        Ok(ModelChunk {
            content: "also kept".into(),
            done: true,
            ..Default::default()
        }),
        Ok(content("after done")),
    ]))
    .await
    .unwrap();
    assert_eq!(reply.content, "keptalso kept");
    assert_eq!(reply.chunks, 2);
}

#[tokio::test]
async fn an_empty_stream_is_an_error_rather_than_an_empty_reply() {
    // An empty answer must never be handed to a parser as if it were one.
    assert!(collect_reply(stream(vec![])).await.is_err());
}

#[tokio::test]
async fn a_stream_error_propagates() {
    let failed = collect_reply(stream(vec![
        Ok(content("partial")),
        Err(ProviderError::Protocol {
            safe_context: "truncated".into(),
        }),
    ]))
    .await;
    assert!(failed.is_err());
}

/// A short answer and an abandoned one assemble into the same text. The only
/// thing that separates them is the terminal chunk, so its absence is a
/// failure rather than the end of a reply.
#[tokio::test]
async fn a_stream_that_ends_without_a_terminal_chunk_is_truncated() {
    let failed = collect_reply(stream(vec![Ok(content("half an ans")), Ok(content("wer"))])).await;
    assert!(matches!(failed, Err(ProviderError::Truncated { .. })));
}

#[tokio::test]
async fn hitting_the_chunk_bound_is_an_error_rather_than_a_short_reply() {
    // A deployment that never stops emitting is bounded, and what was read up
    // to the bound is not an answer: returning it would report a fragment as a
    // complete reply and let it be parsed as an action.
    let chunks: Vec<_> = std::iter::repeat_with(|| Ok(content("x")))
        .take(pwr_provider::MAX_REPLY_CHUNKS + 1)
        .collect();
    let failed = collect_reply(stream(chunks)).await;
    assert!(matches!(failed, Err(ProviderError::Truncated { .. })));
}

/// A stream that stops producing is not a stream that is slow.
///
/// Measured: a backend stopped serving mid-generation and the run noticed
/// fifteen minutes later, at the client timeout, with the error naming the
/// timeout rather than the silence. An operator told "timed out" goes looking
/// for a slow model; one told "produced nothing" goes looking for the backend.
#[tokio::test(start_paused = true)]
async fn silence_is_reported_as_silence_and_not_as_slowness() {
    // One chunk, then nothing, ever.
    let stream: ModelStream = Box::pin(
        futures_util::stream::once(async {
            Ok(pwr_domain::ModelChunk {
                content: "thinking".into(),
                ..Default::default()
            })
        })
        .chain(futures_util::stream::pending()),
    );

    let failure = collect_reply(stream)
        .await
        .expect_err("a stream that never speaks again must not hang forever");
    let said = failure.to_string();
    assert!(said.contains("fell silent"), "{said}");
    assert!(said.contains("after it had begun"), "{said}");
}

/// Reading the prompt produces nothing however healthy the backend is, so the
/// silence bound must not start until the reply does.
///
/// Measured: a 27B whose backend wrote 2.5 GB of prompt cache over three
/// minutes of prefill was cut off by this bound applied from the start, and a
/// working deployment was reported as gone. LM Studio's own log said what had
/// happened -- "Client disconnected. Stopping generation... (If the model is
/// busy processing the prompt, it will finish first.)" -- and the client that
/// disconnected was us.
#[tokio::test(start_paused = true)]
async fn a_long_prefill_is_not_mistaken_for_a_dead_backend() {
    let stream: ModelStream = Box::pin(futures_util::stream::once(async {
        // Far longer than the silence bound, and entirely before the first
        // chunk: this is the deployment reading, not the deployment stopping.
        tokio::time::sleep(pwr_provider::REPLY_IDLE_TIMEOUT * 4).await;
        Ok(pwr_domain::ModelChunk {
            content: "at last".into(),
            done: true,
            ..Default::default()
        })
    }));
    let reply = collect_reply(stream)
        .await
        .expect("a slow prefill is not a dead backend");
    assert_eq!(reply.content, "at last");
}

/// The counterpart: a reply that is merely slow still arrives. The bound
/// separates gone from slow, and must not turn slow into gone.
#[tokio::test(start_paused = true)]
async fn a_slow_reply_that_keeps_speaking_still_arrives() {
    let stream: ModelStream = Box::pin(futures_util::stream::unfold(0usize, |index| async move {
        if index > 3 {
            return None;
        }
        // Long gaps, but never longer than the idle bound.
        tokio::time::sleep(pwr_provider::REPLY_IDLE_TIMEOUT / 2).await;
        Some((
            Ok(pwr_domain::ModelChunk {
                content: index.to_string(),
                done: index == 3,
                ..Default::default()
            }),
            index + 1,
        ))
    }));

    let reply = collect_reply(stream)
        .await
        .expect("a slow reply is a reply");
    assert_eq!(reply.content, "0123");
}
