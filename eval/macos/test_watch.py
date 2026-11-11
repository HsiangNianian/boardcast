import unittest
from watch import process_tree


class ProcessTreeTest(unittest.TestCase):
    def test_helpers_count_but_other_apps_do_not(self):
        rows = '12 1 100 10\n14 12 30 20\n15 14 40 0\n99 1 900 90\n'
        self.assertEqual(process_tree(rows, 12), {'processes': 3, 'rss_kib_sum': 170, 'cpu_percent_sum': 30.0})
        self.assertIsNone(process_tree(rows, 200))
        self.assertEqual(process_tree('12 1 0 0\n', 12)['rss_kib_sum'], 0)

    def test_invalid_metrics_fail(self):
        for rows in ['12 1 -1 0', '12 1 1 nan', '12 1 1 -2', '12 1 invalid 3']:
            with self.assertRaises(ValueError):
                process_tree(rows, 12)


if __name__ == '__main__':
    unittest.main()
