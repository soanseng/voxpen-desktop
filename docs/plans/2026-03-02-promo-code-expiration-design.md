# Promo Code with License Expiration — Design

**Date:** 2026-03-02
**Status:** Approved

## Goal

Add promo code support so users can get 1 month of free Pro access. After expiration, the app downgrades to Free tier with a one-time gentle notification encouraging Pro purchase.

## Approach: Client-side `expires_at` + Server Validation

Parse `expires_at` from LemonSqueezy API, store locally, check on every tier evaluation. Server-side verification remains as backup. This gives instant, precise expiration without grace period delays.

## LemonSqueezy Dashboard Setup

1. **New variant** on VoxPen Pro product: "1-Month Trial", license length = 1 month
2. **100% discount code** scoped to this variant, with optional `max_redemptions` and `expires_at`
3. Users checkout at $0 → receive license key with `expires_at` set 30 days out

## Backend Changes (Rust)

### `licensing/lemonsqueezy.rs`
- Add `expires_at: Option<String>` to `LsLicenseKey` (ISO 8601 from API, null = perpetual)

### `licensing/types.rs`
- Add `expires_at: Option<i64>` to `LicenseInfo` (Unix timestamp, None = perpetual)

### `licensing/manager.rs`
- `current_tier()`: check `expires_at` before returning tier — if past expiry, return `Free`
- `activate()`: parse `LsLicenseKey.expires_at` ISO 8601 → Unix timestamp, store in `LicenseInfo`
- Existing `verify_if_needed` grace period logic untouched (serves as offline/server backup)

### `src/hotkey.rs`
- Track first-time expiration detection with `AtomicBool`
- On first detection of Pro → Free transition with `expires_at` present: emit `"promo-expired"` event
- Resets on app restart (so user sees it at most once per launch)

## Frontend Changes (React/TypeScript)

### `src/types/settings.ts`
- Add `expires_at: number | null` to `LicenseInfo` interface

### `src/components/Overlay.tsx`
- Listen for `"promo-expired"` event
- Show amber overlay:
  - Title: "試用期已結束" / "Trial ended"
  - Message: "升級 Pro 享受無限語音輸入" / "Upgrade to Pro for unlimited voice input"
  - [升級 Pro] button → opens LemonSqueezy checkout URL
  - [稍後] button → dismisses
- No auto-dismiss (requires user interaction)
- Shows only once per event emission

### `src/components/Settings/LicenseSection.tsx`
- When Pro + `expires_at` present: show "Pro（到期日：2026-04-02）"
- After expiry: normal Free tier display (existing behavior)

### `src/locales/en.json` + `zh-TW.json`
New keys:
- `promoExpiredTitle` / `promoExpiredMessage` / `promoExpiredUpgrade` / `promoExpiredDismiss`
- `licenseExpiresAt`

## User Flow

```
User gets Promo Code
  → LemonSqueezy Checkout ($0 with 100% discount)
  → Gets License Key
  → Activates in app → API returns expires_at
  → Pro tier (Settings shows expiry date)
  → 30 days later → current_tier() returns Free
  → First hotkey press → Overlay: "Trial ended, upgrade to Pro"
  → Dismissed → normal Free tier usage
```

## What Does NOT Change

- `verify_if_needed` grace period logic (untouched, serves as backup)
- Usage tracking (Free tier daily limits apply naturally after downgrade)
- Deactivate flow (still works for manual deactivation)
- Existing perpetual Pro licenses (`expires_at = null` → no expiration check)
- Tray menu (naturally shows Free status after downgrade)

## Files Changed

| File | Change |
|------|--------|
| `licensing/lemonsqueezy.rs` | `LsLicenseKey` + `expires_at` field |
| `licensing/types.rs` | `LicenseInfo` + `expires_at` field |
| `licensing/manager.rs` | `current_tier()` expiry check, `activate()` parse |
| `src/hotkey.rs` | First-time expiry detection → emit event |
| `src/components/Overlay.tsx` | Listen `"promo-expired"`, show overlay |
| `src/components/Settings/LicenseSection.tsx` | Show expiry date |
| `src/types/settings.ts` | `LicenseInfo` type + `expires_at` |
| `src/locales/en.json` + `zh-TW.json` | 6 new i18n keys |
