# Opens rom2altsound's HTML pages in real browsers (Chromium and Firefox, headless) and
# reports every JavaScript error: the listening page of a pack, and the catalog site
# (docs/). Built on Microsoft's official Playwright image (browsers included).
FROM mcr.microsoft.com/playwright/python:v1.63.0-noble
# The image holds the browsers; the Python package goes with their exact version.
RUN python3 -m pip install --no-cache-dir --break-system-packages playwright==1.63.0
COPY check_pages.py /usr/local/bin/check_pages.py
ENTRYPOINT ["python3", "/usr/local/bin/check_pages.py"]
