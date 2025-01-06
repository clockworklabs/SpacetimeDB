from .. import Smoketest
import time

class EnergyFlow(Smoketest):

    def test_energy_balance(self):
        """Test getting energy balance."""

        self.new_identity()
        self.publish_module()

        out = self.spacetime("energy", "balance")
        self.assertIn('{"balance":"', out)
