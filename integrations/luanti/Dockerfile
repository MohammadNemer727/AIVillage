# Immutable official Luanti server image for the headless bridge smoke environment.
# Update this digest only after reviewing the corresponding upstream image manifest.
FROM ghcr.io/luanti-org/luanti@sha256:db682caec367977ac2661c81e3cdbd1f1dbbae596f6fa7d791c0031107d59947

COPY games/ai_village /usr/local/share/luanti/games/ai_village
COPY ci/minetest.conf /etc/luanti/minetest.conf
ENTRYPOINT ["luantiserver"]
CMD ["--gameid", "ai_village", "--world", "/world"]
