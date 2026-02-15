"""Exception classes for the Ouroboros SDK"""


class SdkError(Exception):
    """Base SDK error"""

    pass


class NetworkError(SdkError):
    """Network-related error"""

    def __init__(self, message: str):
        super().__init__(f"Network error: {message}")


class TransactionFailedError(SdkError):
    """Transaction failed error"""

    def __init__(self, message: str):
        super().__init__(f"Transaction failed: {message}")


class MicrochainNotFoundError(SdkError):
    """Microchain not found error"""

    def __init__(self, microchain_id: str):
        super().__init__(f"Microchain not found: {microchain_id}")


class InsufficientBalanceError(SdkError):
    """Insufficient balance error"""

    def __init__(self, required: int, available: int):
        self.required = required
        self.available = available
        super().__init__(f"Insufficient balance: required {required}, available {available}")


class InvalidSignatureError(SdkError):
    """Invalid signature error"""

    def __init__(self):
        super().__init__("Invalid signature")


class AnchorFailedError(SdkError):
    """Anchor failed error"""

    def __init__(self, message: str):
        super().__init__(f"Anchor failed: {message}")


class InvalidConfigError(SdkError):
    """Invalid configuration error"""

    def __init__(self, message: str):
        super().__init__(f"Invalid configuration: {message}")
