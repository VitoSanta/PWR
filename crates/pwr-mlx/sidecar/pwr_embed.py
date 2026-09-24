"""PWR's embedding sidecar: text in, unit vectors out, on MLX.

A separate process from the engine on purpose. The engine's prompt cache is
worth ~100 s of prefill per step (backlog D.E2E-21), and a second model in the
same request loop would queue behind generation or stall it. This one holds a
small encoder (`intfloat/multilingual-e5-small` by default, 0.5 GB, MIT) and
answers in milliseconds.

Protocol, one JSON object per line:

    -> {"kind": "query" | "passage", "texts": ["...", ...]}
    <- {"vectors": [[f32, ...], ...], "dimensions": N}
    <- {"error": "..."}

It never touches the network: the model must already be in the HuggingFace
cache (fetched once, by `scripts/setup-mlx.sh` or by hand), and the hub is put
in offline mode before anything is imported. Measured 2026-09-23 on twelve
documentation requests (C.22): e5-small fused with the section BM25 was the
best ranking of every arm tried, at 5 ms per request.

Environment:
- `POORAI_EMBED_MODEL`: a HuggingFace id or a local path (default e5-small).
- `POORAI_EMBED_POOLING`: `mean` (e5's convention, default) or `cls` (bge-m3's).
"""
import json
import os
import sys

# Before any hub import: a model that is not cached is an error to report,
# never a download to start.
os.environ.setdefault("HF_HUB_OFFLINE", "1")
os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")

DEFAULT_MODEL = "intfloat/multilingual-e5-small"
# e5 was trained with these prefixes and ranks worse without them; other
# models are given none.
PREFIXES = {
    "intfloat/multilingual-e5-small": {"query": "query: ", "passage": "passage: "},
    "intfloat/multilingual-e5-base": {"query": "query: ", "passage": "passage: "},
}
MAX_LENGTH = 512
BATCH = 16


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


class Encoder:
    def __init__(self, name, pooling):
        import mlx.core as mx
        from mlx_embeddings.utils import load

        self.mx = mx
        self.name = name
        self.pooling = pooling
        self.model, self.tokenizer = load(name)
        self.prefixes = PREFIXES.get(name, {})

    def embed(self, kind, texts):
        mx = self.mx
        prefix = self.prefixes.get(kind, "")
        vectors = []
        for start in range(0, len(texts), BATCH):
            chunk = [prefix + text for text in texts[start:start + BATCH]]
            inputs = self.tokenizer.batch_encode_plus(
                chunk, return_tensors="mlx", padding=True, truncation=True, max_length=MAX_LENGTH
            )
            out = self.model(inputs["input_ids"], attention_mask=inputs["attention_mask"])
            if self.pooling == "cls":
                first = out.last_hidden_state[:, 0]
                embeds = first / mx.linalg.norm(first, axis=-1, keepdims=True)
            else:
                embeds = out.text_embeds
            mx.eval(embeds)
            vectors.extend(embeds.tolist())
        return vectors


def main():
    name = os.environ.get("POORAI_EMBED_MODEL", DEFAULT_MODEL)
    pooling = os.environ.get("POORAI_EMBED_POOLING", "mean")
    try:
        encoder = Encoder(name, pooling)
    except Exception as error:  # noqa: BLE001 -- reported, not raised
        emit({"error": f"could not load {name} offline: {error}"})
        return 1
    emit({"ready": True, "model": name, "pooling": pooling})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
            kind = request.get("kind", "passage")
            texts = request["texts"]
            if kind not in ("query", "passage") or not isinstance(texts, list):
                raise ValueError("expected kind query|passage and a list of texts")
            vectors = encoder.embed(kind, [str(text) for text in texts])
            emit({"vectors": vectors, "dimensions": len(vectors[0]) if vectors else 0})
        except Exception as error:  # noqa: BLE001 -- one bad request is not the end
            emit({"error": str(error)})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
