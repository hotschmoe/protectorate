use chrono::{TimeZone, Utc};
use protectorate_desk_policy::{
    evaluate, parse_chord, parse_window, Inputs, Policy, Ready, Reason, ScreenState,
};
use std::time::Duration;

#[test]
fn windows_include_start_exclude_end_and_cross_midnight() {
    assert_eq!(parse_window("22:00-06:00").unwrap(), (1320, 360));
    for bad in ["", "23:00", "24:00-01:00", "01:00-01:00", "03:5-04:00"] {
        assert!(parse_window(bad).is_err(), "{bad}");
    }
    let policy = Policy::new(
        "22:00-06:00",
        Duration::from_secs(60),
        "Asia/Shanghai",
        true,
    )
    .unwrap();
    assert!(policy.in_window(Utc.with_ymd_and_hms(2026, 10, 22, 14, 0, 0).unwrap()));
    assert!(policy.in_window(Utc.with_ymd_and_hms(2026, 10, 22, 21, 59, 0).unwrap()));
    assert!(!policy.in_window(Utc.with_ymd_and_hms(2026, 10, 22, 22, 0, 0).unwrap()));
    assert!(Policy::new("00:00-23:59", Duration::ZERO, "", false).is_err());
    assert!(Policy::new("00:00-23:59", Duration::from_secs(1), "No/Such_Zone", false).is_err());
}

#[test]
fn availability_checks_ready_window_idle_and_lock_in_order() {
    let policy = Policy::new("22:00-06:00", Duration::from_secs(60), "UTC", true).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 22, 23, 0, 0).unwrap();
    let mut input = Inputs {
        now,
        last_input: now - chrono::Duration::seconds(61),
        ready: Ready::Interactive,
        screen: ScreenState::Unlocked,
    };
    assert_eq!(
        evaluate(Some(&policy), input_with(&input)).reason,
        Reason::Available
    );
    input.screen = ScreenState::Locked;
    assert_eq!(evaluate(None, input_with(&input)).reason, Reason::Locked);
    assert_eq!(
        evaluate(Some(&policy), input_with(&input)).reason,
        Reason::Locked
    );
    input.last_input = now - chrono::Duration::seconds(30);
    assert_eq!(
        evaluate(Some(&policy), input_with(&input)).reason,
        Reason::UserActive
    );
    input.now = now - chrono::Duration::hours(4);
    assert_eq!(
        evaluate(Some(&policy), input_with(&input)).reason,
        Reason::OutsideWindow
    );
    input.ready = Ready::Unsupported;
    assert_eq!(
        evaluate(Some(&policy), input_with(&input)).reason,
        Reason::Unsupported
    );
    input.ready = Ready::NotInteractive;
    assert_eq!(
        evaluate(None, input_with(&input)).reason,
        Reason::NotInteractive
    );
}
fn input_with(input: &Inputs) -> Inputs {
    Inputs {
        now: input.now,
        last_input: input.last_input,
        ready: input.ready,
        screen: input.screen,
    }
}

#[test]
fn key_chords_map_named_function_and_literal_keys() {
    let chord = parse_chord(" Ctrl + Shift + Esc ").unwrap();
    assert_eq!(
        chord.modifiers.iter().map(|k| k.vk).collect::<Vec<_>>(),
        [0x11, 0x10]
    );
    assert_eq!(chord.key.vk, 0x1b);
    assert_eq!(parse_chord("win+f24").unwrap().key.vk, 0x87);
    assert_eq!(parse_chord("plus").unwrap().key.vk, 0xbb);
    assert!(parse_chord("left+ctrl").is_err());
    for bad in ["", "ctrl+", "f25", "ctrl++s", "ctrl+unknown"] {
        assert!(parse_chord(bad).is_err(), "{bad}");
    }
}
