#!/bin/sh
# Linker shim installed as ld.lld (both over rustc's gcc-ld/ld.lld and over
# /usr/bin/ld.lld — collect2 resolves `-fuse-ld=lld` from PATH, not only
# from the -B compiler path) so every lld invocation goes through here; the
# real linker is moved aside as /usr/bin/ld.lld-real by the Dockerfile.
#
# rustc emits the cdylib's exported symbols into a linker version script,
# and the bash-builtin export_name attributes contain colons (e.g.
# `:args_builtin_load`). lld >= 20 (and GNU ld, and rust-lld) reject an
# UNQUOTED colon there with "syntax error in VERSION script" — the old
# "system lld accepts it" workaround silently died when rust:1-alpine
# moved to lld 22. Quoted symbol strings are standard version-script
# syntax, so quote the offending lines in place and delegate to the real
# lld. Semantics (which symbols are exported) are unchanged.
for a in "$@"; do
  case "$a" in
    --version-script=*)
      f="${a#--version-script=}"
      [ -w "$f" ] && sed -r -i -e 's/^([[:space:]]*)([^"{}[:space:]]*:[^;[:space:]]*);/\1"\2";/' "$f"
      ;;
  esac
done
exec /usr/bin/ld.lld-real "$@"
