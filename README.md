# m87pro-tool

**한국어** | [English](README.en.md)

**Nodal Studio M87 Pro** 키보드를 Windows에서 설정하는 작은 비공식 도구입니다.
제조사 설정 프로그램을 대신하는 가벼운 프로그램 두 개로 이루어져 있습니다.

| 프로그램 | 설명 |
|---|---|
| `m87-gui.exe` | 창 버전. 한국어와 영어를 지원합니다(프로그램 안에서 언어 선택). |
| `m87.exe` | 명령줄 도구. 인자 없이 실행하면 마우스로 쓸 수 있는 전체 화면 터미널 화면이 열립니다. 영어만 지원합니다. |

두 프로그램 모두 다음을 할 수 있습니다.

- 조명 효과를 켜고 색·밝기·속도를 정하거나, 키마다 다른 색을 지정
- 기본 레이어와 Fn 레이어의 키 기능 바꾸기 (다른 키, 미디어 키, 마우스 버튼, 매크로)
- 키보드가 직접 재생하는 매크로 만들기
- Windows 키 잠금, 절전 시간 같은 키보드 설정 바꾸기
- 배터리 잔량 보기 (2.4GHz 수신기)
- 전체 설정을 프로필로 저장·불러오기·내보내기·가져오기
- 키보드를 공장 초기 상태로 되돌리기

설정과 프로필은 `%APPDATA%\m87pro-tool` 폴더에 일반 파일로 저장됩니다.

이 프로그램은 Nodal Studio M87 Pro 전용입니다. 라이선스와 이 안내는 프로그램의
정보 화면에서도 볼 수 있습니다(`m87-gui`: 왼쪽 메뉴의 정보, `m87`: 화면 위쪽 About 버튼).

> 이 프로젝트는 Nodal Studio 및 Sonix와 관계가 없으며, 이들이 보증하거나 지원하지 않습니다.
> 사용에 따른 책임은 사용자에게 있습니다.

## 지원하는 연결

| 연결 | USB ID | 지원 |
|---|---|---|
| USB 케이블 | `0C45:8006` (Sonix) | 지원 |
| 2.4GHz 수신기 | `0C45:FEFE` | 지원 |
| 블루투스 | — | 지원 안 함 (블루투스로는 키보드를 설정할 수 없습니다) |

## 내려받기

[Releases](../../releases) 페이지에서 `m87-gui.exe`와 `m87.exe`를 받으세요. 설치 없이 바로
실행되는 단일 파일이며, 따로 설치할 런타임도 없습니다(C 런타임을 정적으로 포함).

## 명령줄 예시

```
m87                      # 전체 화면 인터페이스 열기
m87 ping                 # 키보드가 응답하는지 확인
m87 battery              # 배터리 잔량 (2.4GHz 수신기)
m87 effects              # 조명 효과 목록
m87 keys                 # 키 이름 목록
m87 light --help         # 조명 옵션
m87 remap --help         # 키 매핑 옵션
m87 profile list         # 저장된 프로필
m87 --verbose ...        # 주고받는 모든 패킷 출력
```

키보드에 쓰는 명령은 `--dry-run`을 붙이면 보내지 않고 패킷만 보여 줍니다.
전체 목록은 `m87 --help` 또는 `m87 <명령> --help`로 볼 수 있습니다.

## 알려진 제한

- **블루투스** 연결로는 설정할 수 없습니다. USB 케이블이나 2.4GHz 수신기를 쓰세요.
- 키보드에서 **Fn+Backspace**를 누르면 조명 전체가 꺼집니다(다시 누르면 켜짐). 꺼져 있는 동안에는
  이 도구로 조명을 바꿔도 보이지 않습니다.
- **방향키 위 조명 바**는 키보드 자체 단축키로만 조절됩니다. 이를 바꾸는 명령은 알려지지 않았습니다.
- 제조사 앱의 음악 반응·실시간 조명 모드는 구현하지 않았습니다.

## 만든 방법

이 도구는 작성자가 가진 하드웨어와의 상호 운용을 위해 처음부터 새로 작성했습니다. 설정
프로토콜은 제조사 앱을 분석해 알아낸 뒤 실제 키보드에서 확인했으며, 내용은
[`docs/protocol.md`](docs/protocol.md)에 정리되어 있습니다.
[`crates/m87-proto/src/layout.rs`](crates/m87-proto/src/layout.rs)의 키 위치는 제조사 앱의
키보드 배치 파일에서 생성했습니다. 제조사의 코드나 실행 파일은 포함되어 있지 않습니다.

## 소스에서 빌드

Rust 1.80 이상과 MSVC 툴체인(`x86_64-pc-windows-msvc`)이 필요합니다.

```
cargo build --release
cargo test --workspace
```

결과물은 `target\release\m87-gui.exe`와 `target\release\m87.exe`입니다.

작업 공간 구성:

| 크레이트 | 역할 |
|---|---|
| `m87-proto` | 설정 프로토콜 패킷 만들기 (입출력 없음) |
| `m87-hid` | 장치 찾기와 HID 통신 (Win32 직접 호출) |
| `m87-core` | 저장 상태, 프로필, 전송 계획 |
| `m87-app` | `m87.exe` (명령줄과 터미널 화면) |
| `m87-gui` | `m87-gui.exe` (Win32 창 화면) |

## 코드 서명 정책 (Code signing policy)

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).
(SignPath.io가 무료 코드 서명을, SignPath Foundation이 인증서를 제공합니다.)

이 저장소의 GitHub Actions 워크플로가 태그된 소스 코드로 빌드한 배포 파일만 서명합니다.
다른 곳에서 빌드한 파일은 이 인증서로 서명하지 않습니다.

팀 역할:

- 커미터·리뷰어: [rity73](https://github.com/rity73)
- 승인자: [rity73](https://github.com/rity73)

모든 서명 요청은 승인자가 직접 승인합니다.

### 개인정보 보호

이 프로그램은 다른 네트워크 시스템으로 어떤 정보도 보내지 않습니다. USB로 키보드하고만
통신하며, 설정과 프로필은 사용자의 PC에만 저장합니다.

## 라이선스

다음 중 하나를 골라 따를 수 있습니다.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

따로 밝히지 않는 한, 이 프로젝트에 포함되도록 제출한 기여는 Apache-2.0 라이선스의 정의에 따라
추가 조건 없이 위와 같은 이중 라이선스를 따릅니다.
