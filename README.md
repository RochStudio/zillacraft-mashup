# ZillaCraft Mashup

ZillaCraft's kaiju loose in a Modern Warfare 2 and Minecraft world.

This is a fork of **[2010 Rust Rewrite Mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) by chasmlol**,
which brought Modern Warfare 2, Skate 3 and Minecraft together in one game on
[IW4L](https://github.com/vladtrc/iw4L), a from-scratch Rust rewrite of MW2. Everything
below "The base game" is chasmlol's and IW4L's work. This fork adds the kaiju from
RochStudio's Minecraft mod ZillaCraft, and more to fight them with.

## How to play

### What you need

- A 64-bit **Windows 10 or 11** PC.
- **Call of Duty: Modern Warfare 2 (2009)** for PC, installed. Only the Steam version is tested. No game files come with this download: the game reads them from your copy.
- An **internet connection the first time** you play. The game downloads Minecraft's own files from Mojang then (about 125 MB). You don't need Minecraft installed.
- Optional, only for Skate 3 mode: the **Xbox 360 version of Skate 3**, extracted (its `default.xex` with the `data` folder beside it), and a controller. Everything else works without it.

### Step by step

1. **Download** `ZillaCraft-Mashup-windows-x64.zip` from the [latest release](https://github.com/RochStudio/zillacraft-mashup/releases/latest).
2. **Extract it**: right-click the zip, choose **Extract All**, and put the folder somewhere you can write to, such as `Documents`. Not in `Program Files`.
3. **Start the game**: open the extracted `ZillaCraft-Mashup` folder and double-click **`iw4l.exe`**.
   If Windows says "Windows protected your PC", click **More info**, then **Run anyway**. It warns about any program that isn't signed.
4. **Show it your MW2**: it finds a Steam copy by itself and asks you to confirm it. Otherwise, select your MW2 folder (the one with `iw4mp.exe` and the `zone` folder in it).
5. **Answer the Skate 3 question**: choose **No**, unless you have Skate 3 for Xbox 360 extracted. Then choose **Yes** and select its `default.xex`.
6. **Go to the Minecraft world**: at the main menu, choose **Create Game**, pick the **Minecraft** tab in the map list, select **overworld** and start. The first time, the game has to download Minecraft's files before the map opens; give it a minute.
   From then on, double-clicking **`Minecraft World.bat`** in the same folder takes you straight there.
7. **Bring in a kaiju**: press the **`` ` ``** key (under Esc) to open the console, type `kaiju godzilla` and press **Enter**. Godzilla drops in 60 blocks in front of you. Close the console with **`` ` ``** again, and fight.
   The others are `kaiju zilla`, `kaiju kong`, `kaiju kingkong` and `kaiju dragon`; `kaiju clear` sends them all away. Type `creative on` first if you'd rather watch than fight.

Steps 4 and 5 happen only the first time. After that, double-click `Minecraft World.bat` (or `iw4l.exe` for the menu).

### If something goes wrong

- **"VCRUNTIME140.dll was not found"**: install the [Microsoft Visual C++ Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe), then start the game again.
- **Wrong MW2 folder, or you want to change your Skate 3 answer**: delete the `.env` file next to `iw4l.exe` and start the game again; it asks again.
- **The Minecraft world doesn't load the first time**: the download starts when the game opens and keeps going in the background. Wait a minute at the menu and start the map again. If it still fails, check your internet connection; the files go into `iw4l-artifacts\minecraft-26.3` next to the game, and deleting that folder downloads them again.

## What this fork adds

- **Godzilla** (Godzilla Minus One): a 50-block boss with his atomic breath, tail swipe, stomp and claws.
- **Zilla**: the 14.5-block Godzilla of Odo Island. It roars when it first sees you, then bites and flings, swipes its tail and stomps.
- **Kong**: a 14.5-block territorial ape. He beats his chest at anyone who comes near and fights whoever comes closer or shoots him: a backhand swipe, a ground slam whose shockwave you can jump, boulders thrown at where you stood, and leaps onto you. Below half health he is enraged: his eyes glow, his bar turns red, and he hits harder and throws two boulders.
- **King Kong**: Kong at Godzilla's size, 50 blocks tall, with the same moves at his scale.
- Each kaiju's model, textures, sounds, moves and stats come from ZillaCraft. A red warning marks the ground before every move, and each one has a boss bar. Godzilla and Zilla crush the terrain they walk through; the apes wade through it.
- Guns, grenades, rockets, killstreaks and the knife all hurt the kaiju, and Minecraft's mobs too.
- **The Ender Dragon**, behaving as vanilla's does: hatch it from its spawn egg and it circles where it hatched, strafes you with fireballs that leave clouds of dragon's breath, lands to roar and breathe fire, charges anyone keeping their distance, and smashes through whatever it flies into. Its head takes full damage; shoot it down for 500 experience.
- **Spawn eggs** hatch their mobs: every mob the Minecraft world has, and the dragon.
- **F5 third person**, as in Minecraft: behind you, in front looking back, or first person.
- **Creative mode**: double-tap jump to fly, nothing hurts you, blocks break instantly and never run out, middle-click picks a block, and Minecraft's creative inventory opens with **E**, with its tabs and search.
- Real inventory icons for every Minecraft item, chests, banners, shulker boxes, heads and shields included.
- Fairer spawns on the Minecraft map: 300 health, and a kaiju walks off after a kill instead of waiting at your spawn.
- **Options → Audio → Output Device** picks which speakers or headset the game plays through.

Console commands (open the console with the backtick key, `` ` ``):

| Command | What it does |
| --- | --- |
| `kaiju godzilla [distance]` | Godzilla drops in that many blocks in front of you (default 60) |
| `kaiju zilla [distance]` | Zilla drops in (default 30) |
| `kaiju kong [distance]` | Kong drops in (default 30) |
| `kaiju kingkong [distance]` | King Kong drops in (default 60) |
| `kaiju dragon [distance]` | The Ender Dragon hatches there (default 40) |
| `kaiju clear` | Every kaiju goes |
| `creative [on\|off]` | Creative mode |
| `give <item> [count]` | Minecraft items, such as `give diamond_block 64` |
| `thirdperson [off\|behind\|front]` | The view F5 cycles |

## Build it yourself

You need [Rust](https://rustup.rs) (the repository pins its version) and, on Windows, the Visual Studio C++ build tools that rustup offers to install. Then, in the repository folder:

```
cargo build --profile play -p launcher
```

The game is `target\play\iw4l.exe`; run it from the repository folder (`target\play\iw4l.exe map minecraft:overworld` goes straight to the Minecraft world). More in [docs/BUILD.md](docs/BUILD.md).

## The base game

Modern Warfare 2, Skate 3 and Minecraft in one game, from
[2010 Rust Rewrite Mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup):

- **MW2**: the multiplayer game, with its maps, guns, killstreaks and HUD.
- **Skate 3 mode**: press **J** on any map to drop onto a board with Skate 3's physics, tricks and grinds.
- **A Minecraft world map**: a real, endless Minecraft 26.3 world, generated by
  [MinecraftOSS](third_party/minecraftoss/README.md), which you fight through with MW2 guns.
- **Controller support**: MW2's console button layouts and aim assist, with every button rebindable.

## The Minecraft map

- Minecraft's own world generation, with its day/night cycle, sky, clouds and lighting. The lighting also falls on your gun.
- Vanilla mobs, both passive and hostile. You can shoot them and they fight back.
- Shoot blocks to break them. Harder blocks take more bullets, scaled by each gun's real MW2 damage and range. Knife them too. Grenades blow up like TNT.
- Broken blocks drop items that you pick up.
- A Minecraft inventory and hotbar restyled in MW2's look. Your guns sit in the hotbar as items, and your MW2 character stands in the inventory window.
- Hold a block to place it. With an empty hand you punch and mine with your bare MW2 hands.
- Minecraft's block, mob and footstep sounds.
- Unlimited ammo and grenades. The Intervention, SPAS-12 and UMP45 are always in your inventory. No time or score limit.
- The minimap shows the Minecraft world around you.
- Skate 3 mode works here too, with full block collision.

You don't need Minecraft installed. The first time the game starts, it downloads Minecraft 26.3's own files (textures, sounds, world data) straight from Mojang's official servers, the same way the Minecraft launcher does, with the `curl` that comes with Windows 10 and 11. That's about 125 MB, into `iw4l-artifacts/minecraft-26.3`. After that it plays offline. Nothing from Minecraft is included in this repository.

### Controls on the Minecraft map

| Keyboard and mouse | Controller | Action |
| --- | --- | --- |
| Mouse / WASD | Sticks | MW2 movement and shooting |
| 1–9, mouse wheel | D-pad left / right | Select a hotbar slot |
| | Y | Swap between the first two hotbar slots |
| E | | Open or close the inventory |
| Q | | Drop the selected item |
| Left click (block or empty hand) | Right trigger | Mine or punch |
| Right click (holding a block) | Left trigger | Place the block |
| Right click (holding a spawn egg) | Left trigger | Hatch its mob |
| F5 | | Third person: behind you, in front of you, first person |
| `` ` `` (under Esc) | | Console (`kaiju godzilla`, `creative on`, ...) |
| J | Click both sticks in | Skate 3 mode |

In creative mode (`creative on`): double-tap jump to fly, jump rises, C or Ctrl sinks, sprint flies faster, middle click picks the block you look at, and **E** opens the creative inventory.

More about skating in [docs/SKATE.md](docs/SKATE.md).

## Controller

Plug in a controller and play; the game follows whichever controller you last touched. Menus work with the D-pad or left stick, **A** to select and **B** to go back.

- **Options → Controller**: button layout (Default, Tactical, Lefty, Bumper Jumper, Bumper Jumper Tactical), stick layout, look and aim-down-sight sensitivity, invert look, response curve, aim assist, vibration and stick deadzones.
- **Options → Controls**: every action shows its key and its controller button. Select one and press a key to rebind the keyboard, or a controller button to rebind the controller.
- **Aim assist** works as it does in MW2 on console: **Standard** slows your aim over an enemy and follows them as they move; **Full** also pulls your aim onto the nearest enemy when you aim down the sight. It's off with the mouse.
- Click both sticks in to switch to and from Skate 3 mode, as **J** does.

## Known issues

- The skateboard is invisible on some maps.

## Credits

- [2010 Rust Rewrite Mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) by chasmlol: the game this fork is built on, with MW2, Skate 3 mode and the Minecraft world together (Apache-2.0).
- [IW4L](https://github.com/vladtrc/iw4L) by vladtrc and contributors: the MW2 rewrite everything runs on (Apache-2.0, see [NOTICE](NOTICE)).
- [MinecraftOSS](third_party/minecraftoss/README.md): the Rust Minecraft engine behind the world, mobs and items.
- ZillaCraft by RochStudio: the kaiju's models, textures, sounds and behaviour. Its files in `crates/kaiju/assets` are not covered by this repository's Apache-2.0 licence; all rights reserved (see [NOTICE](NOTICE)).
- Godzilla and all related characters are trademarks of Toho Co., Ltd.; Kong and King Kong belong to their respective owners. This is an unofficial, non-commercial fan project, not affiliated with or endorsed by any of them.
- Minecraft is a trademark of Mojang Studios, and its files are downloaded from Mojang, not redistributed here. Modern Warfare 2 and Skate 3 belong to their owners. You need your own copies. This project isn't affiliated with any of them.
