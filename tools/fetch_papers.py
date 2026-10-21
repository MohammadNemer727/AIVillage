#!/usr/bin/env python3
"""Download the papers cited in docs/prior-art.md into papers/ for local reading.

Only openly available copies are fetched, in this order: the cited PDF (arXiv, OpenReview,
open-access publishers), the open-access copy Europe PMC holds for a DOI (as PDF, or as full-text
XML from its API when the PDF is not served to scripts), and an arXiv preprint whose title matches
exactly. Project pages that are not papers are saved as HTML. Sites that refuse automated access
are not worked around; those papers are listed, not fetched. The papers are not redistributed:
papers/ is ignored by git, and this script rebuilds it.

Usage: python3 tools/fetch_papers.py [--force]
"""

from __future__ import annotations

import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ElementTree
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "docs/prior-art.md"
OUTPUT = ROOT / "papers"
SECTION = re.compile(r"^## Research since 2024.*?(?=^## )", re.S | re.M)
ENTRY = re.compile(r"\[\*(?P<title>[^*]+)\*\]\((?P<url>https?://[^)\s]+)\)")
USER_AGENT = "ai-village-paper-fetcher/1.0 (local research copies)"
PAUSE_SECONDS = 1.0
WEB_PAGES = ("oasis-model.github.io", "pub.sakana.ai", "tore.tuhh.de")
ATOM = "{http://www.w3.org/2005/Atom}"


def request(url: str) -> tuple[bytes, str, str]:
    """Returns (body, content type, final URL)."""
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "*/*"})
    with urllib.request.urlopen(req, timeout=60) as response:
        return response.read(), response.headers.get("Content-Type", ""), response.geturl()


def slug(title: str) -> str:
    words = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
    return words[:80].rstrip("-")


def cited_doi(url: str) -> str | None:
    if match := re.match(r"https?://(?:doi\.org|www\.science\.org/doi)/(?P<doi>10\.\S+)", url):
        return match["doi"]
    return None


def europe_pmc_open_id(doi: str) -> str | None:
    """PMC id of an open-access copy of `doi`, if Europe PMC holds one."""
    query = urllib.parse.quote(f'DOI:"{doi}"')
    body, _, _ = request(
        "https://www.ebi.ac.uk/europepmc/webservices/rest/search"
        f"?query={query}&format=json&resultType=core"
    )
    for result in json.loads(body).get("resultList", {}).get("result", []):
        if result.get("isOpenAccess") == "Y" and result.get("pmcid"):
            return result["pmcid"]
    return None


def normalized(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", title.lower()).strip()


def arxiv_preprint(title: str) -> str | None:
    """PDF URL of an arXiv preprint whose title matches `title` exactly, ignoring punctuation."""
    words = re.sub(r"[^A-Za-z0-9 ]+", " ", title).split()
    query = urllib.parse.quote("ti:" + " AND ti:".join(words))
    body, _, _ = request(f"https://export.arxiv.org/api/query?search_query={query}&max_results=5")
    for entry in ElementTree.fromstring(body).iter(f"{ATOM}entry"):
        found = entry.findtext(f"{ATOM}title") or ""
        link = entry.findtext(f"{ATOM}id") or ""
        if normalized(found) == normalized(title) and "/abs/" in link:
            return link.replace("/abs/", "/pdf/").replace("http://arxiv.org", "https://export.arxiv.org")
    return None


def doi_candidates(doi: str) -> list[str]:
    candidates = []
    if doi.startswith("10.1371/"):
        candidates.append(f"https://journals.plos.org/plosone/article/file?id={doi}&type=printable")
    if doi.startswith("10.3389/"):
        candidates.append(f"https://www.frontiersin.org/articles/{doi}/pdf")
    if doi.startswith("10.1038/s4"):
        candidates.append(f"https://www.nature.com/articles/{doi.split('/', 1)[1]}.pdf")
    if doi.startswith("10.1088/"):
        candidates.append(f"https://iopscience.iop.org/article/{doi}/pdf")
    if doi.startswith("10.3390/"):
        candidates.append(f"https://doi.org/{doi}")  # MDPI landing page; "/pdf" appended below
    return candidates


def candidates(url: str) -> list[str]:
    """Open PDF locations to try for a cited URL, best first."""
    if match := re.match(r"https?://arxiv\.org/abs/(?P<id>[\w.\-/]+)", url):
        return [f"https://export.arxiv.org/pdf/{match['id']}"]
    if match := re.match(r"https?://openreview\.net/forum\?id=(?P<id>[\w\-]+)", url):
        return [f"https://openreview.net/pdf?id={match['id']}"]
    if match := re.match(r"https?://elifesciences\.org/reviewed-preprints/(?P<id>\d+)", url):
        return [f"https://elifesciences.org/reviewed-preprints/{match['id']}.pdf"]
    if match := re.match(r"https?://www\.nature\.com/articles/(?P<id>[\w\-]+)$", url):
        return [f"https://www.nature.com/articles/{match['id']}.pdf"]
    if doi := cited_doi(url):
        return doi_candidates(doi)
    return []


def fetch_pdf(url: str) -> bytes | None:
    for candidate in candidates(url):
        try:
            body, _, final = request(candidate)
            if not body.startswith(b"%PDF") and "mdpi.com" in final:
                body, _, _ = request(final.rstrip("/") + "/pdf")
        except (urllib.error.URLError, TimeoutError, ValueError):
            continue
        finally:
            time.sleep(PAUSE_SECONDS)
        if body.startswith(b"%PDF"):
            return body
    return None


def fetch(url: str, title: str) -> tuple[bytes, str, str] | None:
    """Returns (content, file extension, status) for the best open copy, or None."""
    host = urllib.parse.urlparse(url).hostname or ""
    if host in WEB_PAGES:
        try:
            body, _, _ = request(url)
            return body, "html", "saved web page"
        except (urllib.error.URLError, TimeoutError, ValueError):
            return None
    if body := fetch_pdf(url):
        return body, "pdf", "downloaded"
    try:
        if (doi := cited_doi(url)) and (pmcid := europe_pmc_open_id(doi)):
            body, _, _ = request(
                f"https://www.ebi.ac.uk/europepmc/webservices/rest/{pmcid}/fullTextXML"
            )
            if body.lstrip().startswith(b"<?xml"):
                return body, "xml", "full text (Europe PMC XML)"
        time.sleep(PAUSE_SECONDS)
        if (preprint := arxiv_preprint(title)) and (body := fetch_pdf_url(preprint)):
            return body, "pdf", "downloaded (arXiv preprint version)"
    except (urllib.error.URLError, TimeoutError, ValueError, ElementTree.ParseError):
        pass
    return None


def fetch_pdf_url(url: str) -> bytes | None:
    try:
        body, _, _ = request(url)
    except (urllib.error.URLError, TimeoutError, ValueError):
        return None
    finally:
        time.sleep(PAUSE_SECONDS)
    return body if body.startswith(b"%PDF") else None


def main() -> int:
    force = "--force" in sys.argv[1:]
    section = SECTION.search(SOURCE.read_text())
    if not section:
        print("research section not found in docs/prior-art.md", file=sys.stderr)
        return 1
    entries = {}
    for match in ENTRY.finditer(section.group(0)):
        entries.setdefault(match["url"], match["title"].strip())
    OUTPUT.mkdir(exist_ok=True)
    rows = []
    for url, title in entries.items():
        existing = [path for path in OUTPUT.glob(f"{slug(title)}.*") if path.suffix != ".md"]
        if existing and not force:
            status, name = "already present", existing[0].name
        elif found := fetch(url, title):
            body, extension, status = found
            name = f"{slug(title)}.{extension}"
            (OUTPUT / name).write_bytes(body)
        else:
            status, name = "not openly available", ""
        print(f"{status:36} {title}")
        rows.append((title, url, status, name))
    lines = [
        "# Downloaded papers",
        "",
        "Generated by `tools/fetch_papers.py` from `docs/prior-art.md`. Local copies only: this",
        "folder is ignored by git. Papers marked unavailable need institutional access.",
        "",
        "| Paper | Status | File |",
        "| --- | --- | --- |",
    ]
    lines += [f"| [{title}]({url}) | {status} | {file} |" for title, url, status, file in rows]
    (OUTPUT / "INDEX.md").write_text("\n".join(lines) + "\n")
    saved = sum(1 for row in rows if row[3])
    print(f"\n{saved} of {len(rows)} cited works saved in {OUTPUT.relative_to(ROOT)}/")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
