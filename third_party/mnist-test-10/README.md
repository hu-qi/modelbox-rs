# Independently labeled MNIST sample provenance

- Local subset: `../../benchmarks/fixtures/mnist-test-10.csv`, ten labeled test rows (each label + 784 uint8 grayscale intensities).
- Source: `https://github.com/makeyourownneuralnetwork/makeyourownneuralnetwork/blob/master/mnist_dataset/mnist_test_10.csv`
- Upstream blob SHA: `2eb1befad4869b1b824d9158df4e0f273fd86097`
- Upstream repository license: GPL-2.0, full license text copied into [LICENSE](LICENSE). The independently licensed fixture remains separate from Apache-2.0 implementation code.
- Fixed labels in file order: 7, 2, 1, 0, 4, 1, 4, 9, 5, 9.
- Original authors of the MNIST dataset retain their rights; the mirror labels are externally sourced and **not model outputs**.

This subset is tiny. Do not report an extrapolated test-set accuracy or claim statistical significance.
The model processes these images with uint8-to-float32 normalization `/255` and no resizing/inversion.
