from .. import Smoketest, requires_anonymous_login
import time

class EnergyFlow(Smoketest):

    @requires_anonymous_login
    def test_energy_balance(self):
        """Test getting energy balance."""

        self.new_identity()
        self.publish_module()

        out = self.spacetime("energy", "balance")
        self.assertRegex(out, '{"balance":"-?[0-9]+"}')
