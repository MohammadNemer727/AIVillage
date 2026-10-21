# Fedora 44 package pin

Fedora 44 does not currently ship packages renamed `luanti`. Its Luanti 5.x packages retain the
historical `minetest` names. Repository metadata checked on 2026-10-21 offers:

```
minetest             5.17.0-1.fc44
minetest-server      5.17.0-1.fc44
minetest-data-common 5.17.0-1.fc44
```

Install only from an interactive user terminal (this project never handles a sudo password):

```sh
sudo dnf install minetest-5.17.0-1.fc44 minetest-server-5.17.0-1.fc44 minetest-data-common-5.17.0-1.fc44
sudo dnf install 'dnf-command(versionlock)'
sudo dnf versionlock add minetest-5.17.0-1.fc44 minetest-server-5.17.0-1.fc44 minetest-data-common-5.17.0-1.fc44
```

Confirm the installed RPMs and lock with:

```sh
rpm -q minetest minetest-server minetest-data-common
sudo dnf versionlock list
```

The upstream project was renamed Luanti. The smoke harness prefers `luantiserver` when present
and falls back to Fedora's `minetestserver` compatibility executable; the bridge uses the
`minetest.*` Lua API retained by Luanti 5.x.
