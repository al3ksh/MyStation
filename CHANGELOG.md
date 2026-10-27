# Changelog

Each release's section becomes its GitHub Release notes and the notes the
in-app updater shows. `npm run release` refuses to publish a version that has
no section here.

## Unreleased

- Settings → Match Windows power mode to thermal mode: Ultra Performance sets
  Windows to best performance, Optimized to balanced, Cool and Quiet to best
  power efficiency. Plugging in or out keeps the match. The row says when
  Energy Saver holds Windows at best power efficiency anyway.

## 0.6.0 — 2026-10-27

- A Spec page lists what the laptop has and supports: the processor, memory
  slots and modules, graphics and storage with their PCIe links, the screen,
  Wi-Fi, Bluetooth version, Thunderbolt, TPM, Secure Boot and the fingerprint
  reader. All of it is read from the laptop itself.
- Connected Bluetooth devices show their battery level under the flyout's
  sensors (mouse, keyboard, headphones and so on), and a notification says when
  one drops to 15%.
- Hardware → USB shows the speed each device runs at, and points out a USB 3
  device stuck at USB 2 speed: try another cable, or a USB 3 port. The list
  updates by itself as devices are plugged in and out.
- Hardware → Displays lists each screen with the resolution and refresh rate
  it runs at and how it's connected. It says when a monitor could run sharper
  or faster: either Windows offers the mode and it's a setting away, or the
  cable or adapter can't carry it.
- The Startup card leads with the time to the desktop and says it's from the
  last full restart. It also shows the firmware time Task Manager calls BIOS
  time, and explains that with fast startup on, a shutdown and power-on isn't
  timed by Windows.

## 0.5.2 — 2026-10-18

- The Power page shows where the power goes. On battery: the CPU, the NVIDIA
  GPU and the rest of the laptop (screen, SSD, Wi-Fi), adding up to the total
  drain. On AC: the CPU, the GPU, the power going into the battery and the
  adapter's rating.

## 0.5.1 — 2026-10-16

- The touchpad no longer lags every 30 seconds. The app used to re-read the
  BIOS that often, and each read froze the CPU for a few seconds. It now
  reads the BIOS once at startup.

## 0.5.0 — 2026-10-13

- The app is now MyStation, and takes the name of the laptop it runs on:
  MyPrecision, MyXPS or MyLatitude. Updating from MyPrecision moves the
  sign-in task over and removes the old install. Settings and history are
  kept.
- A strict content security policy: the app loads nothing but its own code.

## 0.4.0 — 2026-10-12

- Battery profiles are now called Desk and Remote.
- Choose integrated or NVIDIA graphics per app on the Power page.
- The tray icon shows up reliably after boot.
- Fixes to the flyout strip, sensor dot spacing and the thermal tile label.

## 0.3.0 — 2026-10-09

- Hardware page: drive health and wear forecast, startup apps and boot
  times, and USB devices with the battery draw each one adds.
- Power page: which apps keep the NVIDIA GPU awake, battery use by app and
  sleep drain.
- BIOS settings page through Dell Command | Configure.
- Links to the Dell Command downloads when they are missing.
- The tray flyout opens at once.

## 0.2.0 — 2026-10-09

- Thermal mode by power source, a battery profile schedule and protection
  when the laptop stays plugged in for a long time.
- Open the Dell warranty and support page for the service tag.
- Windows show only once their data has loaded, so content no longer pops in.

## 0.1.0 — 2026-10-09

- First release: tray flyout and full window with battery charge limits,
  thermal modes, battery health and charts, and signed in-app updates.
