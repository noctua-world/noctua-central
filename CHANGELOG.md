# Changelog

What each release of this repository adds, changes, fixes, speeds up, secures and removes, newest first (`NOCTUA.md §22.4`). `cargo xtask release` writes this file whole from the conventional commits between the release tags every time it makes a release, so an edit by hand is gone at the next one.

## v0.1.0

### Added

- judge the history, release each channel, and pin every action (#3) (7e14375)
- **ci:** release each channel from its own workflow, and pin every action (079a1f7)
- **xtask:** compute, write and publish a channel's release (eaa97bd)
- **xtask:** judge the history in the gate (a9082e0)
- build and check on Linux, macOS and Windows (#2) (6a16346)
- **nc-platform:** keep the local database and the blobs private on Windows (a0c33d2)
- add the foundations and the local kernel (a79eb10)

### Fixed

- **release:** run the gate with no secret, and judge edited pull requests (#5) (9cdc63a)
- **xtask:** name the suites a system stands aside, and say what holds (ae45121)
- **xtask:** read the paths of every commit the run's merge reaches (3efd525)
- **xtask:** judge an edited pull request, and measure the merge made for the run (fcb5242)
- **release:** hold each install whole, and title a carry-back by hand (1fdfaca)
- **release:** gate in a job with no secret, and mint the token after it (ef7b15e)
- **xtask:** read a written value's next line as that value, not as a header (9c1cac3)
- **xtask:** open a block scalar wherever YAML writes its header (b9893fd)
- **xtask:** read a workflow's line by where it stands, not by what it holds (15caa2a)
- **xtask:** read every step past its dash, its quotes and its spaces (5dad358)
- **xtask:** read every line of a workflow where YAML ends it (341c832)
- **xtask:** read every key of a workflow wherever YAML writes it (998fe02)
- **xtask:** hand the release's authority back as owned sentences (a170f70)
- **xtask:** hold the release workflow whole, and its authority to it alone (4d14b61)
- **xtask:** name CI's cargo commands as the verbs they are (add26df)
- **xtask:** put back every file a refused release wrote (f68d92b)
- **xtask:** hand the release token to gh alone, and carry a release back once (eec0476)
- **xtask:** judge before the release commit, and queue the head it moves (5cf99ab)
- **xtask:** carry a release back on this repository's own pull request (d80f5a3)
- **xtask:** read a suite's spelling from the binary path cargo wrote (4402435)
- **nc-platform:** hand `mklink /J` each path with Windows' separator (30cc75a)
- **xtask:** read a file's role from where its spelling puts it (88a7052)
- **xtask:** read a backslash as a separator only in Windows' spelling (c060636)
- **xtask:** judge a file CI reaches in every role its spellings give it (271d670)
- **xtask:** read each file CI reaches once, whatever it is called (af0277c)
- **xtask:** refuse to run the checkout script under a Windows sh (475a3b6)
- **xtask:** read the formatter's roots and cargo's suites with either separator (283dd02)
- read paths, sockets and shells as macOS and Windows hand them over (505e064)
- **nc-platform:** ask a path being deleted again before refusing it on Windows (3d3f8f8)
- **xtask:** move the running gate aside before the test build links over it (71559cc)
- **nc-platform:** run the Windows refusals, and judge them as Unix does (80b0acf)
- **xtask:** tell every cargo the gate starts when the run is offline (5158fc6)
- **xtask:** hold the fetch to --offline in the verdict (414ed3f)
- **xtask:** fetch the packages the lockfile names before reading the graph (95f25df)
- **xtask:** skip the repository's own .git beside the workspace manifest (ac43614)
