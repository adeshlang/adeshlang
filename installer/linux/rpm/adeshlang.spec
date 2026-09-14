%{!?with_toolchain:%global with_toolchain 0}

Name:           adeshlang
Version:        0.3.0
Release:        1%{?dist}
Summary:        AdeshLang compiler, package manager, language server, and editor
License:        AdeshLang License v2.0
URL:            https://github.com/adeshlang/adeshlang
Requires:       glibc >= 2.28

%description
AdeshLang is a programming language toolchain with AOT compilation support.

%prep

%build

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_prefix}/lib/adeshlang
cp -a "%{_adeshlang_stage}/." %{buildroot}%{_prefix}/lib/adeshlang/
mkdir -p %{buildroot}%{_sysconfdir}/profile.d
cat > %{buildroot}%{_sysconfdir}/profile.d/adeshlang.sh <<'PROFILE'
# AdeshLang environment (managed by the adeshlang package)
export ADESH_HOME="/usr/lib/adeshlang"
export ADESH_TOOLCHAIN="/usr/lib/adeshlang/toolchain/llvm"
export ADESH_CLANG="/usr/lib/adeshlang/toolchain/llvm/bin/clang"
export ADESH_LLC="/usr/lib/adeshlang/toolchain/llvm/bin/llc"
export ADESH_MLIR_OPT="/usr/lib/adeshlang/toolchain/llvm/bin/mlir-opt"
export ADESH_MLIR_TRANSLATE="/usr/lib/adeshlang/toolchain/llvm/bin/mlir-translate"
export PATH="/usr/lib/adeshlang/bin:$PATH"
PROFILE

%post
if [ "$1" -ge 1 ]; then
    for binary in adesh adl als adesh-editor; do
        if [ -x "/usr/lib/adeshlang/bin/$binary" ]; then
            ln -sfn "/usr/lib/adeshlang/bin/$binary" "/usr/bin/$binary"
        fi
    done
fi
%if 0%{?with_toolchain}
if [ "$1" -ge 1 ]; then
    if [ "${ADESH_RPM_USE_SYSTEM_PACKAGES:-${ADESH_USE_SYSTEM_PACKAGES:-0}}" = "1" ]; then
        ADESH_HOME="/usr/lib/adeshlang" /usr/lib/adeshlang/bin/adesh \
            toolchain install --system --use-system-packages
    else
        ADESH_HOME="/usr/lib/adeshlang" /usr/lib/adeshlang/bin/adesh \
            toolchain install --system
    fi
fi
%else
if [ "$1" -ge 1 ]; then
    echo "AdeshLang toolchain not installed by default."
    echo "To install it, run: sudo ADESH_HOME=/usr/lib/adeshlang /usr/lib/adeshlang/bin/adesh toolchain install --system"
    echo "Build this RPM with --with toolchain to install it during package configuration."
fi
%endif

%postun
if [ "$1" -eq 0 ]; then
    for binary in adesh adl als adesh-editor; do
        link="/usr/bin/$binary"
        target="/usr/lib/adeshlang/bin/$binary"
        if [ -L "$link" ] && [ "$(readlink "$link")" = "$target" ]; then
            rm -f "$link"
        fi
    done
    rm -f "%{_sysconfdir}/profile.d/adeshlang.sh"
    # The downloaded LLVM toolchain is generated data; retain it on upgrades,
    # but remove it when the final package is erased.
    rm -rf /usr/lib/adeshlang/toolchain
fi

%files
%{_prefix}/lib/adeshlang
%config(noreplace) %{_sysconfdir}/profile.d/adeshlang.sh

%changelog
* Thu Jan 01 2026 AdeshLang Team <team@adeshlang.org> - 0.3.0-1
- Initial AdeshLang Linux package
