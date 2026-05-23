# Self-Hosted CI Runners

This document covers how to provision self-hosted GitHub Actions runners for
the platform-build workflows under `.github/workflows/`:

| Workflow | Target platform | Runner this doc recommends |
|---|---|---|
| `test.yml` | Linux x86_64 (already done) | Existing `kdf-runner` on pvex1 |
| `build-windows.yml` (P7.3) | Windows x86_64 | Windows VM in Proxmox |
| `build-macos.yml` (P7.2) | macOS x86_64 + ARM64 + Universal | **Apple hardware only** (see § macOS) |
| `build-ios.yml` (P7.4) | iOS aarch64 | Same Apple host as macOS |
| `build-android.yml` (P7.5) | Android aarch64 + armv7 | Linux VM (no emulator needed for build) |
| `dev-build.yml` (P7.6) | Umbrella `workflow_call` | n/a — fans out |

The umbrella workflow `dev-build.yml` calls each child workflow via
`workflow_call`, so once a runner exists for a given child it goes green
without further wiring.

All workflows are currently **`workflow_dispatch` only** (manual trigger
from the GitHub Actions UI) so missing runners cannot break CI.

Hardware assumed: dual EPYC 7702, 128 GB RAM, Proxmox VE host. Nested
virtualization must be enabled on the host (`kvm-amd nested=1` or via
`/etc/modprobe.d/kvm-amd.conf`) for any of the VM-based options below.

---

## 1. Windows runner (P7.3) — provisioned

Identical model to the existing Linux `kdf-runner`. Status as of
`build-windows.yml` commit: VM up, runner registered with the default
GitHub-assigned labels (`self-hosted`, `Windows`, `X64`). The workflow
already targets `[self-hosted, Windows, X64]`.

### Provisioning (reference / for rebuilds)

1. Create a Proxmox VM:
   - Guest OS: Windows 10 22H2 or Windows 11 (either works for `windows-latest` parity).
   - 4 vCPU, 8 GB RAM, 60 GB disk is plenty for incremental Rust builds.
   - VirtIO disk + network drivers from the Fedora `virtio-win` ISO.
2. Inside the VM install:
   - **Git for Windows** — *required on the system PATH (not just the user PATH)*.
     `actions/checkout@v4` falls back to a REST-API tarball download when
     `git --version` is not callable, and the PowerShell `Expand-Archive`
     fallback is broken on dot-prefixed directories like `.cargo/` (it
     errors with `Cannot find path '...\\.cargo\\' because it does not exist`).
     Install with `winget install --id Git.Git -e` or the official
     installer, then either reboot the runner service or restart it so
     the new system PATH is picked up: `Restart-Service actions.runner.*`.
     Verify with `git --version` from a fresh PowerShell window.
   - Visual Studio 2022 Build Tools with the **C++ build tools** workload
     (provides MSVC + Windows SDK — required by the `x86_64-pc-windows-msvc`
     Rust target).
   - `rustup` from <https://rustup.rs/>; install the stable toolchain
     and the MSVC target up front:
     ```powershell
     rustup toolchain install stable --profile minimal
     rustup target add x86_64-pc-windows-msvc
     ```
     The workflow assumes both are already present and does **not**
     re-run `rustup` on every build (avoids per-job network round-trips
     and surfaces missing-toolchain mistakes as a clear failure).
3. Install the runner using the same flow as the Linux box:
   - GitHub repo → Settings → Actions → Runners → "New self-hosted runner",
     pick the Windows tab. Run the PowerShell snippet in an admin shell.
   - Accept the default labels (`self-hosted`, `Windows`, `X64`) — the
     workflow keys off those. If you set a custom label, update
     `build-windows.yml` `runs-on:` to match.
   - Install it as a Windows service (`./svc install` then `./svc start`).
4. `build-windows.yml` is already wired to `[self-hosted, Windows, X64]`.
   Trigger it manually (Actions → Build Windows → Run workflow) for the
   first verification run; once green, add `push`/`pull_request` triggers.

### Caveats

- **Git on PATH is mandatory.** Without it, `actions/checkout@v4` uses a
  REST tarball + PowerShell `Expand-Archive`, which crashes on `.cargo/`.
  Symptom: `Remove-Item : Cannot find path '...\.cargo\' because it does
  not exist` during the Checkout step. See install note above.
- **Shell is `powershell` (Windows PowerShell 5.1), not `pwsh`.** PowerShell
  Core (`pwsh`) is not installed on a stock Windows VM. The workflow uses
  `shell: powershell` to use the built-in interpreter. If you prefer
  `pwsh` for cross-platform parity, install it once via
  `winget install --id Microsoft.PowerShell -e` and switch the workflow
  back to `shell: pwsh`.
- Long Rust paths can hit Windows' `MAX_PATH` limit. Either enable long
  paths via `git config --system core.longpaths true` and the registry
  `LongPathsEnabled = 1`, or build under a short path like `C:\w\`.
- Defender real-time scanning slows compilation a lot. Add the runner's
  work directory to the exclusion list.

---

## 2. macOS runner (P7.2) and iOS runner (P7.4)

**There is no fully legal way to run macOS in a Proxmox VM on x86 hardware.** 
Apple's macOS Software License Agreement permits installation
only on "Apple-branded computer". Running macOS in a non-Apple VM
violates that license, regardless of how technically feasible it is.

This matters for any non-personal use (CI for a public/private repo with
contributors, distributing binaries, anything commercial).

### Honest options, in order of preference

#### Option A — buy or rent a Mac mini (recommended)

The cheapest path that is both legal and reliable.

- Apple Silicon Mac mini (M2 base) ≈ $599 new, ≈ $400 refurbished. ARM64
  natively, x86_64 builds via Rosetta 2, Universal binary via `lipo`.
- Install macOS, install Xcode (free), install `rustup`, install the
  GitHub Actions runner (`runner-osx-arm64.tar.gz`), register with label
  `[self-hosted, macOS, ARM64]`.
- Same Mac handles **both** P7.2 (macOS) **and** P7.4 (iOS). iOS builds
  require Xcode + a macOS host; once you have one, you have both.
- Switch `build-macos.yml` and `build-ios.yml` `runs-on:` to your label.
- For iOS Simulator runtime tests (not currently in the workflow) Xcode's
  bundled simulator is enough; no provisioning profile required for sim.
- For real-device or App Store distribution: Apple Developer Program
  ($99/year) + provisioning profile + signing identity in the runner's
  keychain. None of that is needed just to produce
  `aarch64-apple-ios/libmm2_bin_lib.a`, which is what `build-ios.yml`
  does today.

#### Option B — MacStadium / MacInCloud / scaleway Apple Silicon (rented Mac)

Pay-per-month dedicated Mac mini hosted by a third party. Fully legal
(they own the Apple hardware), but adds a recurring cost and a network
hop. Pricing is in the $50–$120/month range for an M-series mini. Worth
it if you don't want hardware in your home/office.

#### Option C — OSX-KVM / OpenCore in Proxmox (the EULA-violating path)

Technically OSX-KVM (<https://github.com/kholia/OSX-KVM>) does run macOS
Sonoma/Sequoia x86_64 inside a KVM guest on Linux/Proxmox via OpenCore.
EPYC 7702 has the necessary CPU features (SSE4.2, AVX2). Setup is
non-trivial: custom OVMF firmware, OpenCore bootloader, virtio-iommu
quirks, GPU passthrough or no Metal acceleration, no native iCloud.

What this gets you:

- macOS x86_64 only. No ARM64, no Universal binary build path.
- `lipo` "universal" binaries built only from the x86_64 slice would be
  pointless — the workflow explicitly produces both architectures.
- Apple Silicon iOS simulators won't run (they need ARM64 macOS host).
- The host VM is fragile: macOS point releases occasionally break OpenCore
  or virtio drivers, requiring the OpenCore config to be re-tuned.

What this costs:

- Violates the macOS EULA. For private internal use the practical
  enforcement risk is negligible, but you can never legally distribute
  binaries built this way to third parties.
- iOS provisioning/signing for App Store distribution from such a host is
  in an even greyer area.

**Recommendation:** if the goal is distributable cross-platform builds,
do not use Option C. If the goal is purely "see whether the code
compiles on macOS", a single refurbished Apple Silicon Mac mini in your
office (Option A) is the lowest-friction path and unlocks iOS for free.

### Wiring the runner once you have a Mac

```bash
# On the Mac, after rustup + Xcode are installed:
mkdir actions-runner && cd actions-runner
curl -O -L https://github.com/actions/runner/releases/download/v2.X.Y/actions-runner-osx-arm64-2.X.Y.tar.gz
tar xzf ./actions-runner-osx-arm64-*.tar.gz
./config.sh --url https://github.com/<owner>/<repo> \
            --token <runner-token-from-Settings/Actions/Runners> \
            --labels self-hosted,macOS,ARM64
./svc.sh install
./svc.sh start
```

Then in `build-macos.yml`:

```yaml
jobs:
  build:
    runs-on: [self-hosted, macOS, ARM64]
```

Same label scheme works for `build-ios.yml`.

For the matrix `x86_64-apple-darwin` job on an Apple Silicon host, Rosetta 2
handles execution and `rustup target add x86_64-apple-darwin` handles the
toolchain — no second runner needed.

---

## 3. Android runner (P7.5) — fully supported, easy

**You do not need an Android emulator to build the `.so`.** The current
workflow uses `cargo-ndk` against the NDK toolchain, which produces
`libmm2_bin_lib.so` for `arm64-v8a` and `armeabi-v7a` purely via
cross-compilation on a Linux x86_64 host. No emulator, no AVD, no KVM
acceleration required.

### Provisioning (build-only, what the workflow needs today)

1. Create a Proxmox VM:
   - Guest OS: Ubuntu 22.04 LTS or 24.04 LTS.
   - 4 vCPU, 8 GB RAM, 40 GB disk. Disk needs to fit the NDK
     (~3 GB) plus Cargo's target dir.

2. Inside the VM:
   ```bash
   sudo apt update
   sudo apt install -y curl unzip openjdk-17-jdk-headless build-essential pkg-config libssl-dev
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source $HOME/.cargo/env
   cargo install --locked cargo-ndk
   # Install Android NDK r25c (matches the workflow's expected layout):
   curl -L -o ndk.zip https://dl.google.com/android/repository/android-ndk-r25c-linux.zip
   unzip ndk.zip -d $HOME
   export ANDROID_NDK_ROOT=$HOME/android-ndk-r25c
   echo "export ANDROID_NDK_ROOT=$HOME/android-ndk-r25c" >> ~/.bashrc
   rustup target add aarch64-linux-android armv7-linux-androideabi
   ```
3. Install the runner the same way as the Linux `kdf-runner`. Register
   with label `[self-hosted, linux, android-ndk]`. Make sure the runner
   service inherits `ANDROID_NDK_ROOT` (set it in
   `~/actions-runner/.env`).
4. Switch `build-android.yml`'s `runs-on:` from `ubuntu-latest` to your
   label.

### Provisioning (runtime tests, if/when added)

If you ever want to run the binary in an Android emulator inside CI:

- Enable nested virtualization on the Proxmox host:
  `echo "options kvm-amd nested=1" > /etc/modprobe.d/kvm-amd.conf` then
  reload the module or reboot.
- In the VM config (`/etc/pve/qemu-server/<vmid>.conf`), use
  `cpu: host` (or at minimum a CPU type that exposes `vmx`/`svm`).
- Inside the VM:
  ```bash
  sudo apt install -y qemu-kvm libvirt-daemon-system bridge-utils
  sudo usermod -aG kvm,libvirt $USER
  # Install Android command-line tools, then:
  sdkmanager "system-images;android-34;google_apis;x86_64"
  avdmanager create avd -n test-arm64 -k "system-images;android-34;google_apis;x86_64"
  emulator -avd test-arm64 -no-window -no-audio -gpu swiftshader_indirect
  ```
- Note: ARM64 Android system images run **slowly** on x86 host without
  ARM emulation — for runtime tests prefer the `x86_64` image even
  though our cross-compiled build artifact is `arm64-v8a`. The two are
  separate concerns: the `.so` must be built for the production target
  (ARM), but runtime smoke tests in CI can run an x86_64 system image.

The current `build-android.yml` does not run any emulator. Add this
section only when you decide to add device-runtime tests to the matrix.

---

## 4. iOS Simulator (deferred)

`build-ios.yml` only builds `aarch64-apple-ios` static libraries; it
does not run any tests. If you later want iOS Simulator-based tests:

- Simulator only runs on macOS hosts (covered by the macOS runner).
- Add `aarch64-apple-ios-sim` and `x86_64-apple-ios` to the target
  matrix and uncomment the simulator targets in `build-ios.yml`.
- The workflow can run XCTest bundles via `xcodebuild test` against a
  booted simulator on the same Mac runner.

This costs nothing extra once you have a Mac runner.

---

## 5. Summary — what to do now

If your goal is "have all P7 workflows go green", the minimum-cost
sequence is:

1. **Windows VM in Proxmox** → P7.3 ✅ green. Cost: zero.
2. **Linux VM with NDK in Proxmox** → P7.5 ✅ green. Cost: zero.
3. **One refurbished Apple Silicon Mac mini** → P7.2 + P7.4 ✅ green.
   Cost: ~$400 one-time. Unblocks both macOS and iOS.

Steps 1 and 2 can be done immediately and independently of step 3.

After each runner is online, change the corresponding workflow's
`runs-on:` label and the `[~]` entries in `RELOADED-PLAN.md` § P7 can
flip to `[x]`.
