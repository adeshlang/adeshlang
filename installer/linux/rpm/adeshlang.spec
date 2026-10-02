Name:           adeshlang
Version:        0.3.0
Release:        1%{?dist}
Summary:        AdeshLang compiler, package manager, language server, and editor
License:        AdeshLang License v2.0
URL:            https://adeshlang.org
Requires:       glibc >= 2.28, ca-certificates, tar, xz

%description
AdeshLang is a programming language toolchain with AOT compilation support.
The Adesh native toolchain (native codegen, ADOB object format, adeshlink
linker, and adeshlang runtime) is bundled inside this package; no external
LLVM, Clang, GCC, or MSVC installation is required.

%prep

%build

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_prefix}/lib/adeshlang
cp -a "%{_adeshlang_stage}/." %{buildroot}%{_prefix}/lib/adeshlang/
find %{buildroot}%{_prefix}/lib/adeshlang -type d -exec chmod 0755 {} +
find %{buildroot}%{_prefix}/lib/adeshlang -type f -exec chmod 0644 {} +
find %{buildroot}%{_prefix}/lib/adeshlang/bin -type f -exec chmod 0755 {} +
mkdir -p %{buildroot}%{_sysconfdir}/profile.d
# The native toolchain ships inside the package, so no toolchain variables
# are exported. Users who opt into the external LLVM bridge register it
# separately with `adesh toolchain --external install`.
cat > %{buildroot}%{_sysconfdir}/profile.d/adeshlang.sh <<'PROFILE'
# AdeshLang environment (managed by the adeshlang package)
export ADESH_HOME="/usr/lib/adeshlang"
export ADESH_STD="/usr/lib/adeshlang/std"
case ":$PATH:" in
  *":/usr/lib/adeshlang/bin:"*) ;;
  *) export PATH="/usr/lib/adeshlang/bin:$PATH" ;;
esac
PROFILE

%post
if [ "$1" -ge 1 ]; then
    for binary in adesh adl als adesh-editor adeshlink; do
        if [ -x "/usr/lib/adeshlang/bin/$binary" ]; then
            ln -sfn "/usr/lib/adeshlang/bin/$binary" "/usr/bin/$binary"
        fi
    done
    if [ ! -f "/usr/lib/adeshlang/ai/models/adesh-coder-0.5b-q4_0.gguf" ]; then
        echo "Note: AI neural models are not bundled in this package."
        echo "To download and set up the default local AI coder model (~275 MB), run: adesh ai setup"
    fi
    # Self-verify the bundled native toolchain (fast; no downloads).
    if [ -x /usr/lib/adeshlang/bin/adesh ]; then
        ADESH_HOME="/usr/lib/adeshlang" /usr/lib/adeshlang/bin/adesh toolchain check || true
    fi
fi

%postun
if [ "$1" -eq 0 ]; then
    for binary in adesh adl als adesh-editor adeshlink; do
        link="/usr/bin/$binary"
        target="/usr/lib/adeshlang/bin/$binary"
        if [ -L "$link" ] && [ "$(readlink "$link")" = "$target" ]; then
            rm -f "$link"
        fi
    done
    rm -f "%{_sysconfdir}/profile.d/adeshlang.sh"
    # /usr/lib/adeshlang/toolchain only exists when the user registered an
    # external LLVM bridge there; the native toolchain ships in the package.
    rm -rf /usr/lib/adeshlang/toolchain
fi

%files
%{_prefix}/lib/adeshlang
%config(noreplace) %{_sysconfdir}/profile.d/adeshlang.sh

%changelog
* Thu Jan 01 2026 AdeshLang Team <team@adeshlang.org> - 0.3.0-1
- Initial AdeshLang Linux package
- Bundle the self-contained native toolchain (codegen, adeshlink, ADOB,
  runtime); external LLVM is an opt-in bridge, not an install requirement
