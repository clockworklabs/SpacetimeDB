from .. import Smoketest
import time

class EnergyFlow(Smoketest):

    def test_new_user_flow(self):
        """Test getting energy balance."""

        self.new_identity()
        self.publish_module()

        out = self.spacetime("energy", "balance")
        self.assertEqual(out, '{"balance":"1"}')
