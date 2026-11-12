# M87 Pro HID configuration protocol (reverse-engineered)

Source: Ghidra pseudocode of `DeviceDriver.exe` (32-bit MFC, embedded hidapi), plus
`config.xml`, `layouts/rgb-keyboard.xml`, `language/1033.lan` and SQLite schema strings
in the exe. No traffic was captured and the keyboard was never contacted: everything here
is static analysis.

Confidence tags:

- **[O] Observed**: the constant or layout appears directly in the pseudocode at the cited address.
- **[I] Inferred**: an interpretation of observed bytes (naming, units, purpose).
- **[U] Unknown**: not determinable from the code that was read.

All multi-byte values are little-endian unless stated. "data[n]" means the n-th byte
*after* the report-ID byte.

---

## 1. Devices and transports

| Mode (app `+0x690`/`+0x3e4`) | USB IDs | Interface | Transport |
|---|---|---|---|
| 0 = USB wired | VID 0x0C45 PID 0x8006 | MI_00 | **Feature reports**: 65 bytes = report ID 0x00 + 64 data bytes |
| 2 = 2.4 GHz dongle | VID 0x0C45 PID 0xFEFE | MI_03 | **Output/Input reports**: 33 bytes = report ID 0x00 + 32 data bytes |

[O] `config.xml`; enumeration in `FUN_00441c10`:
- An interface whose feature-report length is 0x41 is used as the wired config device.
- An interface with output length 0x21 and input length 0x21 is used as the dongle. On
  open it is pinged with `FUN_00441940`, and the result is stored as "online" (+0x424).
- An interface with usage_page 0xFFFF and usage 0x0001 is kept as a **notification
  input** handle (`+0x28`). See section 6.

### 1.1 Wired feature transport (`FUN_00441260`, the main one)

- [O] Before each packet the app sleeps `cmd_delaytime` ms (`+0x2c`; the layout XML gives 20).
- [O] If len < 0x42, it sends one packet with `HidD_SetFeature` (report ID 0 + 64 bytes copied from the caller's buffer).
  If `readback` is true it then calls `HidD_GetFeature` (IOCTL 0xB0192) once. **The returned data is ignored.**
  The readback acts as a handshake or flow control, not a checked ack.
- [O] If len ≥ 0x42, it sends len/64 consecutive 64-byte chunks with SetFeature and no
  header of its own. There is a delay before each chunk, and up to 10 retries on failure.
- [O] `FUN_00441450` (used for real-time streaming) is the same, with 2 ms sleeps. A bulk send aborts when it takes more than 100 ms in total. It is used for frames sent at about 60 Hz.
- [O] `FUN_004415f0` (bulk read): SetFeature(cmd), then up to N× GetFeature of 64 bytes, copied
  sequentially into the caller's buffer. It aborts if a read returns ≠ 64 bytes or takes more than 360 ms.

### 1.2 Dongle 32-byte transport (`FUN_004417a0`)

```
byte 0      0x00            report ID                                   [O]
byte 1      CMD             command                                      [O]
byte 2      LEN             payload length (0x1C = 28 max, 0x10 = 16, 0x04 ...) [O]
byte 3      IDX             packet index / sub-index                       [O]
byte 4..31  payload (28 bytes max)                                          [O]
byte 32     CHK             8-bit sum of bytes 0..(len-1) of the caller buffer [O]
```
- [O] The caller passes len 0x41 (or 0x21). The buffer is zeroed, so CHK = (sum of bytes 0..31) & 0xFF.
- [O] `hid_write` 33 bytes. On failure: sleep 10 ms, then one retry. Then sleep 5 ms.
- [O] Ack: up to 20× `hid_read_timeout(33, 5 ms)` with 10 ms sleeps between tries, until
  `reply[0]==CMD && reply[1]==LEN && reply[2]==IDX`. The reply is then copied back over the
  caller's buffer, so any status or data is in reply[3..].
- [O] Multi-packet uploads use a 2 ms sleep between packets.

### 1.3 Common framing conventions

- [O] Every uploaded data block ends with **`AA 55`** (0xAA at block_len-2, 0x55 at block_len-1),
  where block_len = 64 × npackets (wired) or 28 × npackets (dongle).
  For single-packet settings and lighting blocks, the trailer sits at data[0x0E..0x0F] on wired (inside the
  first 16 bytes), at data[0x3E..0x3F] for the wired system-settings block, and at payload[0x0E..0x0F] (= byte 0x12..0x13) on the dongle.
- [I] The dongle payload of single-packet commands = wired data block bytes 0..15, shifted
  to report byte 4. Every field matches 1:1 at offset +4.

---

## 2. Wired command set (feature reports, data[0]=0x04 command header)

The header packet is always `04 CC 00 00 00 00 00 00 NN 00 …` (64 bytes, zero padded).
- data[0] = 0x04 [O]: command class "write/control".
- data[1] = CC: sub-command [O].
- data[8] = NN: **number of 64-byte data packets that follow** [O] (observed values agree with the following payload size in every case).

| CC | Name (inferred) | NN | Readback | Evidence |
|---|---|---|---|---|
| 0x18 | Begin config transaction | – | yes | 0x412580, 0x416560, 0x422320, 0x43ec00, 0x43ed20 [O]; meaning [I] |
| 0x19 | Begin macro transaction | – | yes | 0x424950 [O]; meaning [I] |
| 0x02 | End / apply (commit) | – | yes | every writer [O]; meaning [I] |
| 0xF0 | Post-commit (save to flash / reload?) | – | **no** | 0x416560, 0x422320, 0x43ec00, 0x43ed20 [O]; meaning [U] |
| 0x11 | Write key map, base layer | 9 | yes | 0x416560(param=0) [O] |
| 0x27 | Write key map, Fn layer | 9 | yes | 0x416560(param=1) [O] |
| 0x13 | Write lighting-effect block | 1 | yes | 0x422320, 0x43ec00 [O] |
| 0x15 | Write macro storage | n | yes | 0x424950 [O] |
| 0x17 | Write system settings | 1 (data[2]=0x01 [U]) | yes | 0x412580 [O] |
| 0x20 | Real-time per-key frame (custom light / music) | 3 or 8 | n/a (FUN_00441450) | 0x41f030, 0x429820 [O] |
| 0x23 | Write user-light per-key table | 3 or 9 | yes | 0x43ed20 (dup. 0x42b300) [O] |
| 0xF5 | Read current per-key LED frame | 3 or 9 | bulk read | 0x422100 [O] |

The "3 or 9", "3 or 8" and "0xC0 vs 0x240" variants depend on flag `+0x6b0`: 0 = single-colour
(`singlelight-keyboard.xml`), non-zero = RGB keyboard (`rgb_keyboard="1"`; the M87 Pro is RGB). [O]/[I]

Data packets that follow a header are sent with readback=0 (single packet) or as one bulk send of NN×64
bytes (readback flag ignored in the bulk path) [O].

### 2.1 System settings: `04 18` → `04 17` → data → `04 02` (FUN_00412580) [O]

Header: `04 17 01 00 00 00 00 00 01` (data[2]=0x01 [U], data[8]=1 packet).

Data packet (64 bytes):

| off | value | meaning |
|---|---|---|
| 0 | 0x00 | [O] |
| 1 | 0x01 | [O] constant (version/sub-id?) [U] |
| 2 | 0/1 | checkbox `+0x664` = **disable Alt+Tab** [O] (name from FUN_00411c60) |
| 3 | 0/1 | checkbox `+0x668` = **disable Alt+F4** [O] |
| 4 | 0/1 | checkbox `+0x66c` = **disable Win** [O] |
| 5 | n | config `fn_switch` (`_wtoi`) [O] |
| 6 | n | config `sleep_time` [O] (UI: No sleep / 1 min / 5 min / 30 min; default stored value "2"; numeric encoding [U], probably index 0..3 [I]) |
| 7 | 0x00 | [O] |
| 8 | n | config `key_respondtime` [O] (UI "Key Response Time", 5 "Level" entries; default "2"; units [U]) |
| 0x3E,0x3F | AA 55 | trailer [O] |

Note: the data packet is sent with readback=0. No `04 F0` follows.

Dongle equivalent `CMD 0x07` (section 3).

### 2.2 Lighting effect: `04 18` → `04 13` → data → `04 02` → `04 F0` (FUN_00422320) [O]

Header: `04 13 00 00 00 00 00 00 01`.
Data packet (fields from the `t_light_data` row for the profile and mode, `FUN_00406980`):

| off | field | notes |
|---|---|---|
| 0 | mode | see mode table, section 5 [O] |
| 1 | colour byte 0 (R) | low byte of `color_value` [O]; R/G/B order assumes Win32 COLORREF [I] |
| 2 | colour byte 1 (G) | [O]/[I] |
| 3 | colour byte 2 (B) | [O]/[I] |
| 4..7 | 0 | [O] |
| 8 | colorful | 1 = rainbow/multicolour, 0 = single colour [O] field, [I] meaning |
| 9 | brightness | 0..5 (`brightness_max`=5) [O]/[I] |
| 10 | speed | 0..5 (`speed_max`=5) [O]/[I] |
| 11 | direction | [O] field, values [U] |
| 0x0E,0x0F | AA 55 | [O] |

If mode == 0 (LED off), bytes 1..11 are left 0 [O].
`colorindex`, `config_func` and `reserved` are **not** sent [O].

### 2.3 User lighting (per-key static colours) (FUN_0043ec00 + FUN_0043ed20) [O]

1. `04 18` (rb), `04 13 … NN=1` (rb), then the data packet:
   `80 00 00 00 00 00 00 00 00 BR 00 00 00 00 AA 55`. Mode 0x80 = user lighting, data[9] = brightness (`+0x708`) [O].
   Then `04 02` (rb), `04 F0` (no rb).
2. `04 18` (rb), `04 23 … NN` (rb), with NN=3 (single-colour) or 9 (RGB).
3. Table (sent as one bulk write with readback flag; only the header packets are handshaked):
   - RGB: 0x240 bytes = 144 × 4: entry at `light_index*4` = `[light_index, R, G, B]` [O]
     (`R,G,B` = bytes 0x10,0x11,0x12 of the `t_userlightrgb_data` item; order [I]).
   - Single-colour: 0xC0 bytes, byte[light_index] = 0xFF if the key is lit, else 0 [O].
   - `AA 55` at the last two bytes of the table [O].
   - Index 0 is never written (keys with light_index ≤ 0 are skipped) [O].
4. `04 02` (rb), `04 F0` (no rb).

`light_index` comes from `layouts/rgb-keyboard.xml` (map at `+0x718`, keyed by key code) [O]/[I].

### 2.4 Key remap: `04 18` → `04 11|27` → 9 packets → `04 02` → `04 F0` (FUN_00416560) [O]

- Header `04 11 … NN=9` for the base layer, `04 27 … NN=9` for the Fn layer (the app writes the Fn layer
  only if `+0x6b4`, the fn_layer feature, is set).
- Table 0x240 bytes = 144 entries × 4 bytes at `key_index*4` (`key_index` from the layout XML,
  map `+0x710` keyed by `key_code`). `AA 55` at 0x23E/0x23F. Entries left at `00 00 00 00` = default
  key (DB `macro_type` 4 or no row). Action encoding: section 4.

### 2.5 Macro storage: `04 19` → `04 15` → n packets → `04 02` (FUN_00424950) [O]

- `04 19` (rb). Then build a buffer (max 0xE00):
  - **Directory**: 100 × 4-byte entries at offset 0. Entry i = `[off_lo, off_hi, 0, 0]` = byte offset of
    macro i in this buffer, or `FF FF FF FF` if macro i has no records [O]. The first macro starts at 0x190 [O].
  - **Macro record** at `off`: `[cnt_lo, cnt_hi, 0,0,0,0,0,0]` (8-byte header; the 6 zero bytes are [U],
    `play_times`/`delay_type` from `t_macro_data` are **not** sent [O]), followed by `cnt` 4-byte events.
  - Next macro offset = off + 8 + 4×cnt [O].
  - **Event** (4 bytes, byte3 = type) [O], from `t_macrorecord(type,value,delay_time)`:

    | rec.type/value | bytes | meaning |
    |---|---|---|
    | 2 (key) | `00 00 HID B0` | key down [I]; HID usage from VK via `FUN_00443520` [O] |
    | 3 (key) | `00 00 HID 30` | key up [I] |
    | 4, v=1/2/3 | `00 00 M 90` | mouse button down [I], M = 1 (left) / 4 (middle) / 2 (right) |
    | 5, v=1/2/3 | `00 00 M 10` | mouse button up [I] |
    | after **every** record | `DL DH 00 50` | delay in ms = max(delay_time, 10), 16-bit LE [O] |

    So bit7 of the type byte = press, and 0x50 = delay [I].
  - n = floor(len/64) + (len%64 ? 2 : 1) packets (always at least one spare) [O]. `AA 55` at n×64-2 [O].
- `04 15 … NN=n` (rb), sleep 30 ms, bulk send n×64 bytes, sleep 30 ms, `04 02` (rb). No `04 F0` [O].
- Size guard: (macro count + record count) × 8 ≤ 0xA8C on wired, ≤ 0xE10 on 2.4G [O].

### 2.6 Real-time per-key frame (custom light / music on wired) (FUN_0041f030, FUN_00429820) [O]

`04 20 … NN` with NN=3 (single-colour) or 8 (RGB), via the fast transport, then a bulk write of 0xC0 or 0x200 bytes, then `04 02`.
The frame fill code was eliminated by Ghidra ("unreachable block" warnings), so the per-key layout is **[U]**.
It is probably `[idx,R,G,B]` × 128 like the user-light table [I].
Sent repeatedly while the Custom-light page (state 5) or the Music page (state 6) is visible (timer in FUN_0042ad20) [O].

### 2.7 Read current LED frame (FUN_00422100) [O]

`04 F5 … NN` (NN=3 or 9), then NN× GetFeature of 64 bytes into a buffer, then `04 02` (rb).
- RGB: entry at `light_index*4` = `[?, R, G, B]` → colour R | G<<8 | B<<16 [O].
- Single-colour: byte[light_index] = level 0..255 [O].
Polled while UI state 3 is visible, to animate the on-screen keyboard [I]. Wired only.

---

## 3. 2.4 GHz dongle command set (32-byte output reports, FUN_004417a0)

Layout `00 CMD LEN IDX payload[28] CHK`, with an ack echo as in section 1.2. Dongle writes have **no** begin, end or 0xF0 packets [O].

| CMD | LEN | IDX | payload | purpose | evidence |
|---|---|---|---|---|---|
| 0x02 | 0 | 0 | – | ping / is the keyboard online (reply[0]=0x02) | FUN_00441940 [O], meaning [I] |
| 0x03 | 0x04 | key_index | 4-byte action (section 4) | set one key, base layer | FUN_00417130 [O] |
| 0x0D | 0x04 | key_index | 4-byte action | set one key, Fn layer | FUN_00417130 [O] |
| 0x05 | 0x10 | 0 | wired lighting block bytes 0..15 (mode, R,G,B, …, colorful@+8, bright@+9, speed@+10, dir@+11, AA 55@+14) | lighting effect | FUN_004224d0 [O] |
| 0x05 | 0x10 | 0 | `80 00 00 00 … BR@+9 … AA 55` | enter user-light mode | FUN_0043f190 [O] |
| 0x07 | 0x10 | 0 | wired settings block bytes 0..15 (`00 01 altTab altF4 win fn_switch sleep 00 resp … AA55@+14`) | system settings | FUN_004127f0 [O] |
| 0x09 | 0x1C (last: remainder) | packet # | 28-byte slices of the macro buffer (section 2.5) | macro storage | FUN_00424ed0 [O] |
| 0x0B | 0x1C | 0 | [0]=combo `+0x604`, [1]=combo `+0x61c`, [2]=control `+0x624` value, [3..23]=21 column levels | music-rhythm frame | FUN_00429a60 [O]; field meaning [I]: foremode, backmode, background brightness |
| 0x0F | 0 | 0 | – | factory reset (sent on "Factory reset", followed by a lighting rewrite) | FUN_0042b320 [O], meaning [I] |
| 0x10 | 0x1C (last 0x10) | 0..20 | 28-byte slices of the 0x240 key-map table | key map, base layer (21 packets) | FUN_00416e00 [O] |
| 0x12 | 0x1C (last 0x10) | 0..20 | same | key map, Fn layer | FUN_00416e00(param=1) [O] |
| 0x14 | 0x1C (last 0x18 or 0x10) | 0..6 or 0..20 | 28-byte slices of the 0xC0 or 0x240 user-light table | user-light per-key | FUN_0043f190 [O] |
| 0x1F | 0 | 0 | – | reset key mapping to defaults ("Reset Keyboard") | FUN_0042ad20 case 0xC, enqueued by FUN_004129e0 [O]; meaning [I] |
| 0x20 | 0x01 | 0 | – | battery query; reply[0]=0x20, **reply[3] = battery %** (clamped to 100) | FUN_0042baf0 [O]; polled every 15 s |

Notes:
- Macro on the dongle: n = floor(len/28) + 1 + (len%28 ? 1 : 0) packets. The **last** packet's LEN byte =
  len - 28×floor(len/28) (the remainder, which can be 0) [O]. `AA 55` at n×28-2 [O].
- In the single-key command (0x03/0x0D) the default action written to reset a key is `02 00 <key_code> 00` [O].

---

## 4. Key action encoding (4 bytes; key map tables and dongle 0x03/0x0D)

Built by FUN_00416560 / FUN_004161b0 from `t_key_macro_data(macro_type, macro_value, macro_value2, macro_value3)` [O]:

| DB macro_type | bytes | meaning |
|---|---|---|
| 4 / none | `00 00 00 00` | default (firmware keeps the stock key) [O]/[I] |
| 1 | `05 03 00 00` | special function 3 (likely **disable key**) [I] |
| 2 | `02 MOD KEY 00` | keyboard: MOD = HID modifier bitmask (value2), KEY = HID usage (value). If MOD=0 and KEY is E0..E7, encoded as `02 <bit> 00 00` (LCtrl=01 … RGui=80) [O] |
| 3 | `06 IDX MODE CNT` | macro: IDX = position of the macro in `t_macro_data` (upload order), MODE = value2, CNT = value3 (repeat mode/count [I]) [O] |
| 5 | `01 01 BTN 00` / `01 03 DIR 00` | mouse (corrected from FUN_004161b0, M3): button mask BTN v1 → 01 left, v2 → 04 middle, v3 → 02 right, v4 → 03 (L+R? [U]), v7 → 08 back, v8 → 10 forward; wheel v5 → `01 03 01` up, v6 → `01 03 FF` down [O] |
| 6 | `03 LO 00 00` | consumer usage: v1 0xCD play/pause, v2 0xB7 stop, v3 0xB6 prev, v4 0xB5 next, v5 0xE9 vol+, v6 0xEA vol-, v7 0xE2 mute [O] |
| 12 | `03 LO HI 00` | consumer usage 16-bit via FUN_00417440: 500→0x0194 My Computer, 501→0x0192 Calculator, 502→0x018A Mail, 503→0x0183 Media player, 504-510→0x00CD,B7,B6,B5,E9,EA,E2, 511→0x0223 WWW home, 512→0x0227 refresh, 513→0x0226 stop, 514→0x0224 back, 515→0x0225 forward, 516→0x022A favourites, 517→0x0221 search [O] |
| 7 | `02 MOD KEY 00` shortcuts | v1 Win+D (`02 08 07`), v2 Win+E (`08 08`), v3 Win+L (`08 0F`), v4 Ctrl+W (`01 1A`), v5 Alt+Tab (`04 2B`), v6 Ctrl+C (`01 06`), v7 Ctrl+V (`01 19`), v8 Ctrl+X (`01 1B`); v10 → `02 02 00 00` (LShift), v11 → `02 01 00 00` (LCtrl) [O] |
| 8,9,10,11 | `05 02 ID 00` | host-side action: ID = macro_value (row in `t_key_otherdata`). The keyboard echoes it back as a notification (section 6), and the app runs: 8 = launch program, 9 = open URL, 10 = text/other, 11 = switch profile (FUN_00417260) [O]/[I] |
| 13 | `07 V1 V2 V3` | raw value/value2/value3 passthrough, purpose [U] |

---

## 5. Lighting mode IDs (FUN_004225e0 default rows; names from `1033.lan`) [O]

| mode | name (string id) | config_func |
|---|---|---|
| 0 | LED Off (540) | 0x00 |
| 1 | Static (521) | 0x31 |
| 2 | SingleOn (522) | 0x33 |
| 3 | SingleOff (523) | 0x33 |
| 4 | Glittering (524) | 0x33 |
| 5 | Falling (525) | 0x33 |
| 6 | Colourful (526) | 0x03 |
| 7 | Breath (527) | 0x33 |
| 8 | Spectrum (528) | 0x03 |
| 9 | Outward (529) | 0x33 |
| 10 | Scrolling (530) | 0x3B |
| 11 | Rolling (531), the default mode | 0x37 |
| 12 | Rotating (532) | 0x37 |
| 13 | Explode (533) | 0x33 |
| 14 | Launch (534) | 0x33 |
| 15 | Ripples (535) | 0x33 |
| 16 | Flowing (536) | 0x37 |
| 17 | Pulsating (537) | 0x33 |
| 18 | Tilt (538) | 0x37 |
| 19 | Shuttle (539) | 0x33 |
| 0x80 | User lighting (per-key table) | – |

Defaults per row: brightness = `+0x708` (layout default 5), speed = `+0x70c` (default 3), direction 0,
colorful 1, colorindex 0, color_value 0x0000FF [O].
`config_func` is a UI capability mask. Probable bits: 0x01 brightness, 0x02 speed, 0x04 direction, 0x08 extra direction,
0x10 colour, 0x20 colourful [I]. It is not sent to the device.

---

## 6. Device → host notifications (FUN_0042b480) [O]

The app polls the usage-page 0xFFFF / usage 1 input interface every 200 ms (`hid_read_timeout(65, 10 ms)`):
- `05 A6 FF 01 …` → wireless keyboard connected / online [O] (report ID 5 [I]).
- `05 A6 FE 02 …` → keyboard offline / disconnected [O].
- `05 A6 ID …` → host action ID from a type-8..11 key (section 4): run program, URL, text, or profile switch [O].

---

## 7. App-level sequencing (FUN_0042ad20 work queue) [O]

| event | wired | 2.4G |
|---|---|---|
| 6 write all keys | 0x416560(base) [+ Fn] | 0x416e00(base) [+ Fn] |
| 7 write one key | 0x416560(layer) | 0x417130 (CMD 0x03/0x0D) |
| 8 macros | 0x424950 | 0x424ed0 |
| 9 lighting | 0x422320 | 0x4224d0 |
| 10 user lighting | 0x43ed20 (0x43ec00 + table) | 0x43f190 |
| 11 settings | 0x412580 | 0x4127f0 |
| 12 reset keys | (enqueues 6 with an empty DB) | CMD 0x1F |
| 13 factory reset | rewrites all (macros, keys, light, user light, settings) from DB defaults | CMD 0x0F, then lighting |
| poll (UI state 3/5/6) | 04 F5 read / 04 20 stream / 04 20 stream | – / – / CMD 0x0B |
| poll (2.4G) | – | CMD 0x20 battery every 15 s |

There is **no** firmware-version read, profile-switch command, or flash command in this binary range. The
"upgrade" buttons (FUN_0042b5d0 / FUN_0042b860) only download `FirmwareUpdateTool.zip` /
`Keyboard-Software-Setup.zip` and run them. Firmware version 125 comes only from the layout XML [O].

## 8. Open unknowns

1. Meaning of `04 F0` (sent without readback after keymap, lighting and user-light commits), and of data[2]=1 in the `04 17` header.
2. Contents of the GetFeature readback after header packets: is it an ack or status? The app never checks it.
3. Per-key frame layout of `04 20` (custom light / music, wired): the fill code was lost in decompilation.
4. Units and encodings of `sleep_time`, `key_respondtime`, `fn_switch`, and `direction` values.
5. Macro record header bytes 2..7, and the semantics of `06 IDX MODE CNT` (repeat mode/count).
6. Key action types 1 (`05 03`), 13 (`07 …`), and mouse v4 (`01 03`).
7. Dongle reply payload for anything other than battery and ping (status codes in reply[3..]).
8. 2.4G music fields [0..2] (combo items 0x604/0x61c/0x624).

---

## 9. Runtime observations (2026-11-12, 2.4 GHz receiver, `m87` M1)

Captured with `m87 ping -v` / `m87 battery -v`. These are **observed on the real device**, not inferred.

Receiver HID interfaces (PID FEFE):

| MI | usage page:usage | in / out / feature bytes | use |
|---|---|---|---|
| 0 | 0001:0006 | 9 / 2 / 0 | boot keyboard |
| 1 | 0001:0002 | 6 / 0 / 0 | mouse |
| 2 (col01..05) | 000C:0001, 0001:0080, 0001:0006, 0001:000E, **FFFF:0001** | 3, 2, 16, 9, **4** / 0 / 0 | media, system, NKRO keyboard, ?, **events** (section 6) |
| **3** | **FF60:0061** | **33 / 33 / 0** | **settings port** (section 3) |
| 4 | FF59:0061 | 65 / 65 / 0 | unused by the original app [U] |

Ping (CMD 0x02). The reply carries device data in bytes 4.. (offsets are in the reply *with* report ID):
```
>> 00 02 00 00 00 … 00 02
<< 00 02 00 00 40 00 30 00 45 0C 06 80 26 01 FF FF 01 00 66 01 00 … D6
```
- `45 0C` = VID 0x0C45, `06 80` = PID 0x8006 (the keyboard's wired PID) [O]; `26 01` = 0x0126 (firmware/version?) [U];
  `40 00 30 00`, `FF FF`, `01 00`, `66 01` meaning [U]. The reply ends with its own checksum byte [O].

Battery (CMD 0x20): `<< 00 20 01 00 5C …` → 0x5C = **92 %** [O] (byte 3 of the reply after the report ID, as in section 3).

### 9.1 Wired (USB cable, PID 8006) — first live session

- The settings port was found by the 65-byte feature-report rule and opened normally.
- **Readback is an ack** [O]: after each control packet the feature read returns the same
  `04 CC` bytes with **data[3] = 0x01**, e.g. `04 18 00 01`, `04 13 00 01 … 01` (data[8] echoed).
  The original app ignores it; `m87` now checks it.
- Apply (`04 02`) readback carries extra bytes: `04 02 00 01 07 02` after a brightness-5 effect,
  `04 02 00 01 04 02` after a brightness-2 effect. Meaning of data[4], data[5] [U].
- Colour byte order **R, G, B** confirmed visually (FF0000 = red, 0000FF = blue) [O].
- Brightness 2 is visibly dimmer than 5 [O].
- **Esc lit red while the USB cable is plugged in** = charging indicator, independent of the
  lighting effect and the per-key table (user observation) [O].
- After per-key colours, switching to an effect shows the effect normally; the stored per-key table
  is kept for the next time per-key mode is selected [I].
- The wired settings port is **MI_02, usage FF13:0001** (65-byte feature reports), not MI_00 as
  `config.xml` states. Wired interfaces seen: MI_00 boot keyboard; MI_01 collections 0001:0002,
  0001:0006, 0001:0080, 000C:0001, FFFF:0001 (events); MI_02 FF13:0001 (settings) [O].

### 9.2 Key map and settings — first live session (2.4 GHz receiver, M3)

- `00 1F` (reset keys): acknowledged; keys work normally afterwards and **lighting is unchanged** [O],
  so 0x1F resets the key maps only.
- Fn-layer key map (CMD 0x12, 21 packets) with `fn.Pause = 03 E2 00 00` (mute): Fn+Pause mutes,
  Pause alone unchanged [O]. Writing the table again with all zeros restores the default [O].
- The ack reply for key-map slices echoes CMD/LEN/IDX but returns zero payload [O].
- **Replies are sometimes lost on the 2.4 GHz link** (packet 0x11 of 21 had no reply). Re-sending
  the same packet succeeds; `m87` retries up to 3 times [O].
- Settings (CMD 0x07) `00 01 00 00 01 00 02 00 02` + `AA 55`: Windows key blocked; sending
  `… 00 …` again unblocks it [O].
