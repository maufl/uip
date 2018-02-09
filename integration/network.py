#!/usr/bin/env python2

from time import sleep
from cmd import IDENTCHARS

from mininet.net import Mininet, CLI
from mininet.log import output, error
from mininet.topo import Topo
from mininet.nodelib import LinuxBridge

class CustomCLI(CLI):
    identchars = IDENTCHARS + '-'

    def __init__(self, mininet, additionalNets=None, **params):
        self.networks = [mininet] + (additionalNets or [])
        CLI.__init__(self,mininet,**params)

    def findNode(self, name):
        for network in self.networks:
            if name in network:
                return network.get(name)
        return None

    def do_nodes( self, _line ):
        "List all nodes."
        nodes = [ node for network in self.networks for node in network ]
        nodes = ' '.join( sorted( nodes ) )
        output( 'available nodes are: \n%s\n' % nodes )

    def do_links( self, _line ):
        "Report on links"
        for network in self.networks:
            for link in network.links:
                output( link, link.status(), '\n' )

    def default( self, line ):
        """Called on an input line when the command prefix is not recognized.
           Overridden to run shell commands when a node is the first
           CLI argument.  Past the first CLI argument, node names are
           automatically replaced with corresponding IP addrs."""

        first, args, line = self.parseline( line )

        node = self.findNode(first)
        if node:
            if not args:
                error( '*** Please enter a command for node: %s <cmd>\n'
                       % first )
                return
            rest = args.split( ' ' )
            # Substitute IP addresses for node names in command
            # If updateIP() returns None, then use node name
            rest = [ self.findNode(arg).defaultIntf().updateIP() or arg
                     if self.findNode(arg) else arg
                     for arg in rest ]
            rest = ' '.join( rest )
            # Run cmd on node:
            node.sendCmd( rest )
            self.waitForNode( node )
        else:
            error( '*** Unknown command: %s\n' % line )

class ScopedTopo(Topo):
    numTopos = 0

    def __init__(self, *args, **params):
        self.scopeName = params.pop('scopeName', 'net%d' % ScopedTopo.numTopos)
        ScopedTopo.numTopos += 1
        Topo.__init__(self, *args, **params)

    def addNode(self, name, **opts):
        name = '%s_%s' % (self.scopeName, name)
        Topo.addNode(self, name, **opts)

    def addLink(self, node1, node2, port1=None, port2=None, key=None, **opts):
        node1 = '%s_%s' % (self.scopeName, node1)
        node2 = '%s_%s' % (self.scopeName, node2)
        Topo.addLink(self, node1, node2, port1, port2, key, **opts)

class ResidentialTopo(ScopedTopo):
    def build(self,hosts=2):
        self.addSwitch('s0')
        for i in range(hosts):
            self.addHost('pc%s' % i, ip=None)
            self.addLink('s0', 'pc%s' %i)
        self.addHost('rout', ip='192.168.0.1')
        self.addLink('s0', 'rout')

class ResidentialNet(Mininet):
    processes = []

    def __init__(self, name='home'):
        Mininet.__init__(self,topo=ResidentialTopo(scopeName=name),switch=LinuxBridge,controller=None)
        router = self.getNodeByName('home_rout')
        dnsmasq = router.popen('dnsmasq',  '-C', 'homerouter.conf', '-i',router.intf().name)
        self.processes.append(dnsmasq)
        for name in ['home_pc0', 'home_pc1']:
            host = self.getNodeByName(name)
            dhcpcd = host.popen('dhcpcd', host.intf().name)
            self.processes.append(dhcpcd)

    def stop(self):
        for p in self.processes:
            p.terminate()
            p.wait()
        Mininet.stop(self) 

class IspTopo(ScopedTopo):
    def build(self):
        self.addSwitch('s0')
        self.addHost('rout')
        self.addHost('serv')
        self.addLink('s0', 'rout')
        self.addLink('s0', 'serv')

residential = ResidentialNet()
isp = Mininet(topo=IspTopo(scopeName='isp'),switch=LinuxBridge,controller=None)
home_router = residential.getNodeByName('home_rout')
isp_router = isp.getNodeByName('isp_rout')
isp.addLink(isp_router, home_router)
isp.start()
residential.start()
CustomCLI(residential, additionalNets=[isp])
