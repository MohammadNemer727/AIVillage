FROM registry.fedoraproject.org/fedora:44

# rust-gpu builds SPIRV-Tools as a GNU host library. The repository helper mounts the host's
# pinned cargo-gpu and rustup toolchains into this otherwise minimal native Fedora environment.
RUN dnf -y install gcc gcc-c++ glibc-devel git \
    && dnf clean all
