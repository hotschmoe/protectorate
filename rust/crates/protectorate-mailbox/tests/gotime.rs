use protectorate_mailbox::Time;
use std::time::{Duration, SystemTime};

fn t(secs: i64, nanos: u32) -> Time {
    Time::from_secs_nanos(secs, nanos)
}

#[test]
fn formats_like_go() {
    let table = [
        (Time::GO_ZERO, "0001-01-01T00:00:00Z"),
        (t(-62_135_596_800, 0), "0001-01-01T00:00:00Z"),
        (t(0, 0), "1970-01-01T00:00:00Z"),
        (t(-1, 0), "1969-12-31T23:59:59Z"),
        (t(-1, 999_999_999), "1969-12-31T23:59:59.999999999Z"),
        (t(1_792_411_200, 0), "2026-10-19T12:00:00Z"),
        (
            t(1_792_411_200, 123_456_700),
            "2026-10-19T12:00:00.1234567Z",
        ),
        (
            t(1_792_411_200, 123_456_789),
            "2026-10-19T12:00:00.123456789Z",
        ),
        (t(1_792_411_200, 100_000_000), "2026-10-19T12:00:00.1Z"),
        (t(1_740_614_400, 0), "2025-02-27T00:00:00Z"),
        (t(1_582_934_400, 0), "2020-02-29T00:00:00Z"),
        (t(-2_208_988_800, 0), "1900-01-01T00:00:00Z"),
        (t(2_147_483_648, 0), "2038-01-19T03:14:08Z"),
    ];
    for (time, want) in table {
        assert_eq!(time.format(), want, "{:?}", time);
    }
}

#[test]
fn parses_what_go_writes() {
    let table = [
        ("1970-01-01T00:00:00Z", t(0, 0)),
        ("1969-12-31T23:59:59.999999999Z", t(-1, 999_999_999)),
        (
            "2026-10-19T12:00:00.1234567Z",
            t(1_792_411_200, 123_456_700),
        ),
        ("2026-10-19t12:00:00z", t(1_792_411_200, 0)),
        ("2026-10-19T14:00:00+02:00", t(1_792_411_200, 0)),
        ("2026-10-19T06:30:00-05:30", t(1_792_411_200, 0)),
        ("2026-10-19T12:00:00.5-00:30", t(1_792_413_000, 500_000_000)),
        ("2026-10-19T12:00:60Z", t(1_792_411_260, 0)),
        ("0001-01-01T00:00:00Z", Time::GO_ZERO),
    ];
    for (text, want) in table {
        assert_eq!(Time::parse(text), Some(want), "{:?}", text);
    }
    let round_trip = [
        "1970-01-01T00:00:00Z",
        "1969-12-31T23:59:59.999999999Z",
        "2026-10-19T12:00:00.1234567Z",
        "0001-01-01T00:00:00Z",
    ];
    for text in round_trip {
        assert_eq!(Time::parse(text).unwrap().format(), text);
    }
}

#[test]
fn rejects_malformed_timestamps() {
    let bad = [
        "",
        "2026-10-19",
        "2026-10-19T12:34:56",
        "2026-10-19 12:34:56Z",
        "2026-13-01T00:00:00Z",
        "2026-10-32T00:00:00Z",
        "2026-10-19T24:00:00Z",
        "2026-10-19T12:60:00Z",
        "2026-10-19T12:00:00.Z",
        "2026-10-19T12:00:00+0200",
        "2026-10-19T12:00:00Z ",
        "not a time",
    ];
    for text in bad {
        assert_eq!(Time::parse(text), None, "{:?}", text);
    }
}

#[test]
fn round_trips_through_system_time() {
    let table = [
        t(0, 0),
        t(1_792_411_200, 123_456_789),
        t(-62_135_596_800, 0),
        t(-1, 999_999_999),
    ];
    for time in table {
        let system: SystemTime = time.to_system();
        assert_eq!(Time::from_system(system), time, "{:?}", time);
    }
    let now = Time::now();
    assert!(now.secs() > 1_700_000_000);
    assert!(now.nanos() < 1_000_000_000);
}

#[test]
fn compares_and_adds() {
    let early = t(100, 500_000_000);
    let late = t(200, 250_000_000);
    assert_eq!(
        late.checked_duration_since(early),
        Some(Duration::new(99, 750_000_000))
    );
    assert_eq!(early.checked_duration_since(late), None);
    assert_eq!(
        late.checked_duration_since(late),
        Some(Duration::from_secs(0))
    );
    assert_eq!(
        early.checked_add(Duration::new(100, 600_000_000)),
        Some(t(201, 100_000_000))
    );
    assert!(early < late);
    assert_eq!(Time::GO_ZERO, Time::parse("0001-01-01T00:00:00Z").unwrap());
}
