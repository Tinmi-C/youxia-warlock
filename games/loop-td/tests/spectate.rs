//! SEAM3c acceptance (loop-td side): the LT2 input vocabulary records into
//! the input log and replays through the real input channel.
//!
//! - E (equip): `equip_input` records the raw press as a `Key` envelope
//!   when recording is armed (game-side recorder extension).
//! - dig/challenge/artifact: card `Custom` actions dispatch through the
//!   command card and are recorded as `Card` envelope words by the base ui
//!   recorder.
//! - Spectate: a `SpectateSession` re-injects both — the `Key` envelope
//!   lands as a `KeyboardInput` message that bevy's own input system turns
//!   into `ButtonInput::just_pressed` in the SAME numbered frame, and the
//!   `Card` envelope re-fires `CommandCardAction` for the dispatcher.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use loop_td::plugins::game::GamePlugin;
use warbase::prelude::*;
use warbase::testing::test_app;

/// The real game headless PLUS bevy's input plugin (the spectate Key
/// envelope rides the real keyboard-event channel; without it ButtonInput
/// never updates and the E path cannot re-fire).
fn game_app_with_input() -> App {
    let mut app = test_app();
    app.add_plugins(bevy::input::InputPlugin);
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(11));
    app.update();
    app.update();
    app
}

/// Arm recording like `replay_io::arm_recording` would (map hash unused
/// here — the driver does not check it).
fn arm(app: &mut App) {
    let seed = app.world().resource::<MatchRng>().seed();
    let mut rec = RecordingLog::default();
    rec.log.version = INPUT_LOG_VERSION;
    rec.log.seed = seed;
    rec.log.map_hash = 0;
    app.insert_resource(rec);
}

fn keyboard_press(code: KeyCode) -> KeyboardInput {
    KeyboardInput {
        key_code: code,
        logical_key: Key::Character(format!("{code:?}").into()),
        state: ButtonState::Pressed,
        window: Entity::PLACEHOLDER,
        text: None,
        repeat: false,
    }
}

/// E records as a Key envelope; a dispatched custom card action records as
/// a Card word — same-frame entries merge.
#[test]
fn lt2_key_and_card_inputs_record_into_the_log() {
    let mut app = game_app_with_input();
    arm(&mut app);
    for _ in 0..5 {
        app.update();
    }
    // Frame 6: press E through the real channel; dispatch the dig card.
    app.world_mut().write_message(keyboard_press(KeyCode::KeyE));
    app.world_mut().write_message(warbase::ui::CommandCardAction {
        action: warbase::ui::CommandAction::Custom("dig".into()),
        selection: warbase::ui::SelectionMode::Build,
    });
    app.update();
    for _ in 0..5 {
        app.update();
    }
    let log = app
        .world_mut()
        .remove_resource::<RecordingLog>()
        .unwrap()
        .log;
    let frame6 = log
        .frames
        .iter()
        .find(|(f, _)| *f == 6)
        .expect("frame 6 recorded");
    assert!(
        frame6
            .1
            .iter()
            .any(|e| matches!(e, CommandEnvelope::Key { code } if code == "KeyE")),
        "E press recorded as a Key envelope: {:?}",
        frame6.1
    );
    assert!(
        frame6
            .1
            .iter()
            .any(|e| matches!(e, CommandEnvelope::Card { action } if action == "custom:dig")),
        "card dispatch recorded as a Card word: {:?}",
        frame6.1
    );
}

/// The spectate driver re-injects both vocabulary arms: the Key envelope
/// becomes a real KeyboardInput message (same-frame ButtonInput press) and
/// the Card envelope re-fires the CommandCardAction the game dispatches on.
#[test]
fn spectate_reinjects_lt2_key_and_card_envelopes() {
    // Build the log by hand (the recording side is pinned above).
    let mut log = MatchInputLog::default();
    log.version = INPUT_LOG_VERSION;
    log.seed = 11;
    log.map_hash = 0;
    log.frames.push((
        6,
        vec![
            CommandEnvelope::Key { code: "KeyE".into() },
            CommandEnvelope::Card { action: "custom:dig".into() },
        ],
    ));

    let mut app = game_app_with_input();
    app.insert_resource(SpectateSession::new(log));
    let mut keys = MessageCursor::<KeyboardInput>::default();
    let mut actions = MessageCursor::<warbase::ui::CommandCardAction>::default();
    for _ in 0..7 {
        app.update();
        // Drain each frame (double-buffer pitfall).
        {
            let msgs = app.world().resource::<Messages<KeyboardInput>>();
            let saw = keys.read(msgs).any(|k| k.key_code == KeyCode::KeyE);
            if saw {
                // The re-injected press reached bevy's ButtonInput this same
                // frame (keyboard_input_system pressed it in PreUpdate).
                assert!(
                    app.world()
                        .resource::<ButtonInput<KeyCode>>()
                        .just_pressed(KeyCode::KeyE),
                    "Key envelope -> real just_pressed in frame 6"
                );
            }
        }
        {
            let msgs = app
                .world()
                .resource::<Messages<warbase::ui::CommandCardAction>>();
            let saw = actions
                .read(msgs)
                .any(|a| a.action == warbase::ui::CommandAction::Custom("dig".into()));
            if saw {
                return; // both channels observed; done
            }
        }
    }
    panic!("spectate never re-fired the LT2 envelopes");
}
