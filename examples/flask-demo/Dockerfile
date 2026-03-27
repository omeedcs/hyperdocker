# NOTE: Deliberately suboptimal — COPY before pip install.
# Any source change forces a full pip reinstall.
FROM python:3.11-slim
WORKDIR /app
COPY . .
RUN pip install --no-cache-dir -r requirements.txt
EXPOSE 5000
CMD ["python", "app.py"]
