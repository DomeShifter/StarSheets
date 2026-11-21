# StarSheets in one container: the web app and its file server (server/files.py).
# Built by CI on each release (the site comes from the release's web build, in ./site).
#   docker run -p 8080:8080 -v starsheets:/data ghcr.io/domeshifter/starsheets
FROM python:3.15.2-alpine
RUN adduser -D -u 1000 starsheets && mkdir -p /data && chown starsheets /data
COPY server/files.py /app/files.py
COPY site/ /site/
ENV STORE=/data SITE=/site PORT=8080
EXPOSE 8080
VOLUME /data
USER starsheets
HEALTHCHECK --interval=60s --timeout=5s CMD python3 -c "import urllib.request;urllib.request.urlopen('http://127.0.0.1:8080/healthz',timeout=4)"
CMD ["python3", "-u", "/app/files.py"]
