"""Offline/pinning regressions without a model load or network request."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch
import encoder_assets as assets
import pwr_embed

class EncoderAssets(unittest.TestCase):
    def test_runtime_uses_pinned_cached_snapshot_even_if_id_shadows_a_local_directory(self):
        download = Mock(return_value='/cache/pinned')
        with patch.dict(sys.modules, {'huggingface_hub':types.SimpleNamespace(snapshot_download=download)}), patch.object(assets, 'verify', return_value=Path('/cache/pinned')) as verify, patch.object(Path, 'exists', return_value=True):
            self.assertEqual(assets.resolve_for_runtime(assets.MODEL), Path('/cache/pinned'))
        download.assert_called_once_with(repo_id=assets.MODEL, local_files_only=True, revision=assets.REVISION, allow_patterns=list(assets.MANIFEST['files']))
        verify.assert_called_once_with('/cache/pinned')
    def test_custom_hub_model_is_also_offline(self):
        download = Mock(return_value='/cache/custom')
        with patch.dict(sys.modules, {'huggingface_hub':types.SimpleNamespace(snapshot_download=download)}), patch.object(Path, 'exists', return_value=False):
            self.assertEqual(assets.resolve_for_runtime('owner/custom'), Path('/cache/custom'))
        download.assert_called_once_with(repo_id='owner/custom', local_files_only=True)
    def test_explicit_installer_requests_only_manifest_files(self):
        download = Mock(return_value='/cache/pinned')
        with patch.dict(sys.modules, {'huggingface_hub':types.SimpleNamespace(snapshot_download=download)}), patch.object(assets, 'verify') as verify:
            result = assets.install()
        download.assert_called_once_with(repo_id=assets.MODEL, revision=assets.REVISION, allow_patterns=list(assets.MANIFEST['files']))
        verify.assert_called_once_with('/cache/pinned')
        self.assertEqual(result['files'], 7)
        self.assertFalse(any(name.endswith(('.onnx','.bin')) for name in assets.MANIFEST['files']))
    def test_missing_changed_and_unsafe_files_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); data=b'fixed artifact'; file=root/'config.json'; file.write_bytes(data)
            manifest={'files':{'config.json':{'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}}}
            self.assertEqual(assets.verify(root,manifest),root)
            file.write_bytes(b'wrong artifact')
            with self.assertRaises(ValueError):assets.verify(root,manifest)
            file.unlink()
            with self.assertRaises(OSError):assets.verify(root,manifest)
            with self.assertRaises(ValueError):assets.verify(root,{'files':{'../escape':{}}})
    def test_default_prefixes_and_cache_identity_survive_snapshot_resolution(self):
        mlx=types.ModuleType('mlx'); core=types.ModuleType('mlx.core'); mlx.core=core
        embeddings=types.ModuleType('mlx_embeddings'); utils=types.ModuleType('mlx_embeddings.utils'); utils.load=Mock(return_value=(object(),object())); embeddings.utils=utils
        with patch.dict(sys.modules,{'mlx':mlx,'mlx.core':core,'mlx_embeddings':embeddings,'mlx_embeddings.utils':utils}), patch.object(pwr_embed,'resolve_for_runtime',return_value=Path('/cache/pinned')):
            encoder=pwr_embed.Encoder(assets.MODEL,'mean')
            other=pwr_embed.Encoder(assets.MODEL,'cls')
        self.assertEqual(encoder.prefixes,{'query':'query: ','passage':'passage: '})
        self.assertTrue(encoder.identity.startswith(f'{assets.MODEL}@{assets.REVISION};settings='))
        self.assertNotEqual(encoder.identity,other.identity)
        self.assertEqual(utils.load.call_count,2)
        utils.load.assert_called_with('/cache/pinned')
    def test_embedding_process_forces_offline_before_hub_import(self):
        env=dict(os.environ,HF_HUB_OFFLINE='0',TRANSFORMERS_OFFLINE='0',PYTHONPATH=str(Path(__file__).parent))
        result=subprocess.run([sys.executable,'-c','import pwr_embed,os; print(os.environ["HF_HUB_OFFLINE"],os.environ["TRANSFORMERS_OFFLINE"],__import__("sys").dont_write_bytecode)'],env=env,capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(result.stdout.strip(),'1 1 True')

if __name__ == '__main__':unittest.main()
