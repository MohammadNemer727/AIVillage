# Immutable official Luanti 5.17.0 server image (Alpine) for the headless bridge smoke run.
# Update this digest only after reviewing the corresponding upstream image manifest.
# Build from integrations/luanti: docker build -t ai-village-luanti integrations/luanti
FROM ghcr.io/luanti-org/luanti@sha256:db682caec367977ac2661c81e3cdbd1f1dbbae596f6fa7d791c0031107d59947

# The image ships no games directory, so point the server at the bundled game explicitly.
COPY games /opt/ai_village/games
ENV LUANTI_GAME_PATH=/opt/ai_village/games
ENTRYPOINT ["/usr/local/bin/luantiserver", "--gameid", "ai_village"]
