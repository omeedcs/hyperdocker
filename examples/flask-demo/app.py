from flask import Flask, render_template
from config import Config

app = Flask(__name__)
app.config.from_object(Config)

@app.route("/")
def index():
    return render_template("index.html", message="Hello from Hyperdocker!")

@app.route("/health")
def health():
    return {"status": "ok", "version": "1.0.0"}

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=5000, debug=True)
