# Ballatui

Live baseball in the terminal. A scoreboard, a pitch-by-pitch gamecast, box
scores, a hand-scored-style scorecard and standings, all from the public MLB
Stats API, in a Gruvbox dark look.

## Features

- **Scores**: every game on a day, with live inning, outs and runners,
  probable pitchers before the game and decisions after it. Favorite teams
  are pinned to the top. A side panel shows the selected game's linescore.
- **Gamecast**: linescore, a strike zone with numbered pitches (catcher's
  view, colored by result), count and outs, base runners, the pitcher's line
  and pitch count, the batter's day and season slash line, every pitch of the
  at-bat with type, speed and exit velocity, and a running play log. Live
  games refresh every few seconds.
- **History and replay**: step through any game one pitch or one at-bat at a
  time, or replay it from the first pitch at 0.5x to 10x. Score, bases,
  outs, linescore, scorecard and play log all rewind with it.
- **Box score**: batting and pitching lines, batting/fielding notes and game
  info.
- **Scorecard**: a scorebook page per team, with notation such as `6-4-3 DP`,
  `F8`, `KL`, `FC 6-4`, `E5`, `SF9`, how far each batter got around the
  bases, out numbers, RBIs, pinch runners and extra-inning automatic runners.
- **Plays**: the whole game grouped by half-inning; Enter jumps the gamecast
  to any play.
- **Standings**: division and wild-card tables with last ten, streak and run
  differential.

## Install

Requires a recent Rust toolchain (2024 edition).

```sh
cargo install --path .
ballatui
```

or run from the repo with `cargo run --release`.

```
ballatui --date 2026-07-04     # open the scoreboard on a date
ballatui --game 123456         # open a game by its gamePk
ballatui --paths               # print where the settings file lives
```

## Keys

Press `?` in the app for the full list.

| Where | Keys |
|---|---|
| Everywhere | `1` scores · `2` standings · `3` open game · `?` help · `q` quit |
| Scores | `j`/`k` pick · `h`/`l` day · `H`/`L` week · `[`/`]` previous/next day with games · `t` today · `Enter` open · `r` refresh |
| Game | `c` `b` `s` `p` gamecast · box · scorecard · plays (or `Tab`) |
| | `h`/`l` one pitch back/forward · `[`/`]` previous/next at-bat · `0` first pitch · `f` back to live (or the end) |
| | `Space` replay/pause · `R` replay from the first pitch · `+`/`-` speed |
| | `t` away/home team · `j`/`k`, `Ctrl-d`/`Ctrl-u` scroll · `<`/`>` scroll the scorecard sideways · `Esc` back |
| Standings | `w` divisions ↔ wild card · `j`/`k` scroll |

The mouse wheel scrolls.

## Settings

`config.toml` is created on first run in the platform config directory
(`ballatui --paths` prints it):

```toml
favorites = ["SEA", "CHC"]   # pinned and highlighted
live_poll_seconds = 10
scoreboard_poll_seconds = 30
replay_step_seconds = 1.2    # per step at 1x
terminal_title = true
day_rollover_hour = 6        # before this hour, "today" is still yesterday's games
```

`BALLATUI_API` points the app at another server with the same API (handy
for testing against recorded games).

## How it works

The app keeps the full play-by-play feed of the open game and rebuilds the
state at any point from it: runners are tracked by player through every
movement, so bases, outs, score, per-inning runs, hits and errors, pitcher
tallies and the scorecard all come from the same pass. At the latest point
the official linescore is shown, since a few scoring changes (some errors)
never appear in the play data.

A test (`cargo test -- --ignored`, with `BALLATUI_FIXTURES` set to a folder
of saved `feed/live` responses) checks the rebuilt runs, hits, errors and
inning-by-inning score against the official linescore of each game.

## Data

Game data comes from the MLB Stats API (`statsapi.mlb.com`). It is
copyrighted by MLB Advanced Media and its terms allow individual,
non-commercial, non-bulk use. This project is not affiliated with or
endorsed by MLB.

## License

Licensed under the GNU Affero General Public License v3.0 or later. See [LICENSE](LICENSE).
