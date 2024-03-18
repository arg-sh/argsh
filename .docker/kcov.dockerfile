FROM kcov/kcov

RUN set -eux \
  && apt update \
  && apt install -y git \
  && git clone https://github.com/bats-core/bats-core.git \
  && cd bats-core \
  && ./install.sh /usr/local \
  && cd .. \
  && rm -rf bats-core \
  && apt remove -y git \
  && apt autoremove -y \
  && rm -rf /var/lib/apt/lists/*