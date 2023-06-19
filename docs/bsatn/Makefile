all: bsatn-spec.html bsatn-spec.pdf

%.html: %.tex
	cat katex-header.html >$@
	katex -i $< >>$@

%.pdf: %.html
	wkhtmltopdf $< $@