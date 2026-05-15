# Story 33.4: Extract Hardcoded Toast Timeout Constants

Status: done

## Story

As a developer,
I want toast timeout values defined as named constants,
so that they are shared between the ToastLevel documentation and the timeout dispatch logic.

## Acceptance Criteria

1. **Named constants replace hardcoded values**
   - Given the toast timeout values are currently hardcoded in a match arm in `ui/mod.rs`
   - When the code is inspected
   - Then the timeout values are defined as named constants
   - And the match arm uses the constants instead of literal numbers

2. **Constants defined alongside ToastLevel**
   - Given the `ToastLevel` enum in `state_machine.rs`
   - When a developer reads the enum definition
   - Then a method `default_timeout_seconds()` is defined on `ToastLevel`
   - And each variant returns its timeout value:
     - `Error => 0` (persistent, no auto-dismiss)
     - `Warn => 5` (5 seconds)
     - `Info => 3` (3 seconds)

3. **Constant source of truth**
   - Given the `ToastLevel` enum and its timeout values
   - When the value is used in `ui/mod.rs`
   - Then it references `level.default_timeout_seconds()` instead of a raw number
   - And any other code that needs a toast timeout uses the same method

## Technical Requirements

- Current hardcoded match arm (ui/mod.rs lines 2588-2591):
  ```rust
  crate::mpd::state_machine::ToastLevel::Error => 0,
  crate::mpd::state_machine::ToastLevel::Warn => 5,
  crate::mpd::state_machine::ToastLevel::Info => 3,
  ```
- Move to `state_machine.rs` where `ToastLevel` is defined:
  ```rust
  impl ToastLevel {
      /// Returns the default toast timeout in seconds.
      /// - Error: 0 (persistent, must be manually dismissed)
      /// - Warn: 5 seconds
      /// - Info: 3 seconds
      pub const fn default_timeout_seconds(&self) -> u32 {
          match self {
              ToastLevel::Error => 0,
              ToastLevel::Warn => 5,
              ToastLevel::Info => 3,
          }
      }
  }
  ```
- The `ui/mod.rs` match arm becomes: `level.default_timeout_seconds()`
- The `adw::Toast::set_timeout()` takes a `u32` (seconds) — 0 means persistent.
- No behavioral change — code quality only.

## References
- [Source: epics.md] Epic 33: Notification Router Lifecycle — Story 33.4
- [Source: deferred-work.md] Code review 28-4-notification-router — Toast timeout values hardcoded in match arm
- [Source: src/ui/mod.rs:2588-2591] Hardcoded toast timeout match arm
- [Source: src/mpd/state_machine.rs] ToastLevel enum definition
