#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 scripts/analyze.py
pandoc paper/caprice-manuscript.md --standalone --resource-path=paper -o paper/caprice-manuscript.tex
(cd paper && pdflatex -interaction=nonstopmode -halt-on-error caprice-manuscript.tex > build-pass1.log)
(cd paper && pdflatex -interaction=nonstopmode -halt-on-error caprice-manuscript.tex > build-pass2.log)
pandoc paper/caprice-manuscript.md --standalone --resource-path=paper --mathjax=https://cdn.jsdelivr.net/npm/mathjax@3.2.2/es5/tex-mml-chtml.js --css=paper.css -o paper/caprice-manuscript.html
