use crate::storage::{Participant, Segment};
use serde::Deserialize;
use std::collections::HashMap;

/// Raw language-neutral payload produced by either inference sidecar. It keeps
/// engine speaker keys opaque so Swift/FluidAudio and Rust/sherpa-onnx do not
/// each invent their own ordering, deduplication, or participant policy.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SidecarTranscriptResult {
    pub(crate) language: String,
    #[allow(dead_code)]
    duration_seconds: f64,
    segments: Vec<SidecarSegment>,
}

#[derive(Debug, Clone, Deserialize)]
struct SidecarSegment {
    /// Model-native identity, meaningful only within this transcript.
    speaker: SidecarSpeaker,
    text: String,
    start: f64,
    end: f64,
}

/// macOS already emits contiguous numeric IDs, while Windows emits opaque model
/// keys. Converting both to an internal key lets the host apply one participant
/// policy without requiring a behavioral Swift change.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SidecarSpeaker {
    Key(String),
    Id(u32),
}

impl SidecarSpeaker {
    fn into_key(self) -> String {
        match self {
            Self::Key(key) => key,
            Self::Id(id) => id.to_string(),
        }
    }
}

/// App-facing transcript after shared policy has converted raw model output to
/// the storage schema used by recording metadata and Markdown rendering.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptResult {
    pub(crate) language: String,
    pub(crate) participants: Vec<Participant>,
    pub(crate) segments: Vec<Segment>,
}

/// Canonicalize sidecar output for every platform: chronological segments first,
/// then contiguous speaker IDs in order of first appearance, with one generic
/// participant per distinct raw key. Sidecars remain inference adapters and the
/// user-visible labeling policy has one implementation.
pub(crate) fn normalize_transcript(mut raw: SidecarTranscriptResult) -> TranscriptResult {
    raw.segments.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| a.end.total_cmp(&b.end))
    });

    let mut speaker_ids = HashMap::<String, u32>::new();
    let mut next_speaker_id = 0_u32;
    let segments = raw
        .segments
        .into_iter()
        .map(|segment| {
            let speaker_key = segment.speaker.into_key();
            let speaker = if let Some(id) = speaker_ids.get(&speaker_key) {
                *id
            } else {
                let id = next_speaker_id;
                next_speaker_id += 1;
                speaker_ids.insert(speaker_key, id);
                id
            };

            Segment {
                speaker,
                text: segment.text,
                start: segment.start,
                end: segment.end,
            }
        })
        .collect();
    let participants = (0..next_speaker_id)
        .map(|id| Participant {
            id,
            label: format!("Speaker {}", id + 1),
        })
        .collect();

    TranscriptResult {
        language: raw.language,
        participants,
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_fixture_parses_and_normalizes() {
        let fixture = include_str!("../ariso-stt/shared/fixtures/transcript.json");
        let raw: SidecarTranscriptResult = serde_json::from_str(fixture).unwrap();
        let result = normalize_transcript(raw);

        assert_eq!(result.language, "en");
        assert_eq!(result.participants[0].label, "Speaker 1");
        assert_eq!(result.segments[0].speaker, 0);
    }

    #[test]
    fn sorts_deduplicates_and_assigns_participants_once() {
        let raw = SidecarTranscriptResult {
            language: "en".to_string(),
            duration_seconds: 8.0,
            segments: vec![
                SidecarSegment {
                    speaker: SidecarSpeaker::Key("model-speaker-b".to_string()),
                    text: "Second".to_string(),
                    start: 4.0,
                    end: 5.0,
                },
                SidecarSegment {
                    speaker: SidecarSpeaker::Key("model-speaker-a".to_string()),
                    text: "First".to_string(),
                    start: 0.0,
                    end: 1.0,
                },
                SidecarSegment {
                    speaker: SidecarSpeaker::Key("model-speaker-a".to_string()),
                    text: "Third".to_string(),
                    start: 6.0,
                    end: 7.0,
                },
            ],
        };

        let result = normalize_transcript(raw);

        assert_eq!(
            result
                .segments
                .iter()
                .map(|segment| (segment.text.as_str(), segment.speaker))
                .collect::<Vec<_>>(),
            vec![("First", 0), ("Second", 1), ("Third", 0)]
        );
        assert_eq!(
            result
                .participants
                .iter()
                .map(|participant| (participant.id, participant.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "Speaker 1"), (1, "Speaker 2")]
        );
    }

    #[test]
    fn accepts_the_existing_macos_numeric_speaker_contract() {
        let json = r#"{
            "language":"en",
            "durationSeconds":2.0,
            "participants":[{"id":0,"label":"Speaker 1"}],
            "segments":[{"speaker":0,"text":"hello","start":0.0,"end":1.0}]
        }"#;

        let raw: SidecarTranscriptResult = serde_json::from_str(json).unwrap();
        let result = normalize_transcript(raw);

        assert_eq!(result.participants[0].label, "Speaker 1");
        assert_eq!(result.segments[0].speaker, 0);
    }
}
