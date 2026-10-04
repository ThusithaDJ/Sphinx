# Package manager manifests

Ready-to-submit manifests so people can install Sphinx with a package
manager. Update the version, URLs and SHA-256 hashes for each new release.

## winget (Windows Package Manager)

`winget/manifests/t/ThusithaDJ/Sphinx/<version>/` uses the same folder
layout as [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs).

To submit a new package:

1. Validate the manifest on Windows: `winget validate --manifest packaging\winget\manifests\t\ThusithaDJ\Sphinx\0.1.0`
2. Test-install it: `winget install --manifest packaging\winget\manifests\t\ThusithaDJ\Sphinx\0.1.0`
3. Fork `microsoft/winget-pkgs`, copy the version folder to the same path
   there, and open a pull request. An automated check installs it in a
   sandbox, then a moderator approves it.

Once it's published, users install it with `winget install ThusithaDJ.Sphinx`.

For later releases, [wingetcreate](https://github.com/microsoft/winget-create)
does the update and the pull request in one step:

```
wingetcreate update ThusithaDJ.Sphinx --version 0.1.2 --urls <setup.exe URL> --submit
```

## Scoop

`scoop/sphinx.json` installs the portable exe. Release assets include that
exe from v0.1.2 onwards, so this manifest is a draft until that release
exists: fill in `hash` with the portable exe's SHA-256
(`Get-FileHash <file> -Algorithm SHA256`).

Scoop's main buckets only take well-known apps, so the usual route for a new
app is your own bucket: a public GitHub repo (e.g. `ThusithaDJ/scoop-bucket`)
with this file in a `bucket/` folder. Users then run:

```
scoop bucket add thusithadj https://github.com/ThusithaDJ/scoop-bucket
scoop install sphinx
```

`checkver` and `autoupdate` let Scoop's tooling pick up new releases
automatically.
