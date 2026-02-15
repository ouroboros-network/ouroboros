from setuptools import setup, find_packages

setup(
    name="ouroboros-sdk",
    version="0.3.0",
    packages=find_packages(),
    install_requires=[
        "requests>=2.31.0",
        "pynacl>=1.5.0",
        "typing-extensions>=4.0.0",
    ],
)
