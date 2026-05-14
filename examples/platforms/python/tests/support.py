import unittest


class DemoTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self._current_demo_case = None

    def demo_case(self, case_id: str) -> None:
        self._current_demo_case = case_id

    def fail(self, msg: object = None) -> None:
        if self._current_demo_case is not None:
            text = "" if msg is None else str(msg)
            if "case:" not in text:
                msg = f"{self._current_demo_case}: {text}"
        super().fail(msg)

    def _callTestMethod(self, method):
        try:
            return super()._callTestMethod(method)
        except Exception as error:
            case_id = self._current_demo_case
            if case_id is not None and "case:" not in str(error):
                raise AssertionError(f"{case_id}: {error}") from error
            raise
