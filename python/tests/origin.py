import numpy as np
import pytest
import torch

import equistore
from equistore import Labels, TensorBlock, TensorMap


class TestOrigin:
    def test_different_origins(self):
        """Test that an error is thrown when attempting
        to initialize a TensorMap with TensorBlocks
        with different origins"""
        keys = Labels.arange("dummy", 2)
        block_numpy = TensorBlock(
            values=np.array([[0.0]]),
            samples=Labels.single(),
            components=[],
            properties=Labels.single(),
        )
        block_torch = TensorBlock(
            values=torch.tensor([[0.0]]),
            samples=Labels.single(),
            components=[],
            properties=Labels.single(),
        )
        with pytest.raises(equistore.status.EquistoreError, match="different origins"):
            TensorMap(keys=keys, blocks=[block_numpy, block_torch])
