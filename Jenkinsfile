import org.jenkinsci.plugins.workflow.steps.FlowInterruptedException

String imageDigest(name) {
  sh(returnStdout: true, script: "docker inspect --format='{{index .RepoDigests 0}}' $name | cut -d':' -f 2").trim()
}

node {
  properties([
    disableConcurrentBuilds(),
    githubProjectProperty(displayName: 'SpacetimeDB', projectUrlStr: 'https://github.com/clockworklabs/SpacetimeDB/'),
  ])

  try {
    def GRAFANA_IMAGE_TAG
    def GRAFANA_IMAGE_DIGEST
    def PROMETHEUS_IMAGE_TAG
    def PROMETHEUS_IMAGE_DIGEST
    def SPACETIMEDB_IMAGE_TAG
    def SPACETIMEDB_IMAGE_DIGEST
    stage('Clone Repository') {
      checkout scm
    }

    stage('Build Grafana Image') {
      GRAFANA_IMAGE_TAG="clockwork/spacetimedb_grafana-partner-${env.PARTNER_NAME}"
      def grafana = docker.build("${GRAFANA_IMAGE_TAG}", "packages/grafana")
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        grafana.push()
        GRAFANA_IMAGE_DIGEST = imageDigest("${GRAFANA_IMAGE_TAG}")
        grafana.push("${GRAFANA_IMAGE_DIGEST}")
      }
    }

    stage('Build Prometheus Image') {
      PROMETHEUS_IMAGE_TAG="clockwork/spacetimedb_prometheus-partner-${env.PARTNER_NAME}"
      def prometheus = docker.build("${PROMETHEUS_IMAGE_TAG}", "packages/prometheus")
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        prometheus.push()
        PROMETHEUS_IMAGE_DIGEST = imageDigest("${PROMETHEUS_IMAGE_TAG}")
        prometheus.push("${PROMETHEUS_IMAGE_DIGEST}")
      }
    }

    stage('Build SpacetimeDB Image') {
      SPACETIMEDB_IMAGE_TAG="clockwork/spacetimedb-partner-${env.PARTNER_NAME}"
      def spacetimedb = docker.build("${SPACETIMEDB_IMAGE_TAG}", ". -f crates/spacetimedb-standalone/Dockerfile")
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        spacetimedb.push()
        SPACETIMEDB_IMAGE_DIGEST = imageDigest("${SPACETIMEDB_IMAGE_TAG}")
        spacetimedb.push("${SPACETIMEDB_IMAGE_DIGEST}")
      }
    }

    stage('CLI Prebuild') {
      sh "rm -rf cli-bin"
      sh "mkdir cli-bin"
    }

    stage('All CLI Builds') {
      parallel (
        linux: {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            sh "scp -o StrictHostKeyChecking=accept-new -P 9001 -i '${keyfile}' .jenkins/linux-build.sh jenkins@vpn.partner.spacetimedb.net:linux-build.sh"
            sh "ssh -o StrictHostKeyChecking=accept-new -p 9001 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net bash linux-build.sh ${env.BRANCH_NAME}"
            sh "scp -o StrictHostKeyChecking=accept-new -P 9001 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net:/home/jenkins/SpacetimeDB/target/release/spacetime ./cli-bin/spacetime.linux"
          }
        },
        macos: {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            sh "scp -o StrictHostKeyChecking=accept-new -P 9002 -i '${keyfile}' .jenkins/macos-build.sh jenkins@vpn.partner.spacetimedb.net:/Users/jenkins/macos-build.sh"
            sh "ssh -o StrictHostKeyChecking=accept-new -p 9002 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net bash macos-build.sh ${env.BRANCH_NAME}"
            sh "scp -o StrictHostKeyChecking=accept-new -P 9002 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net:/Users/jenkins/SpacetimeDB/target/spacetime-universal-apple-darwin-release ./cli-bin/spacetime.macos"
          }
        },
        windows: {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            sh "scp -o StrictHostKeyChecking=accept-new -P 9003 -i '${keyfile}' .jenkins/windows_build.bat jenkins@vpn.partner.spacetimedb.net:windows_build.bat"
            sh "ssh -o StrictHostKeyChecking=accept-new -p 9003 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net windows_build ${env.BRANCH_NAME}"
            sh "scp -o StrictHostKeyChecking=accept-new -P 9003 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net:C:/Users/jenkins/SpacetimeDB/target/release/spacetime.exe ./cli-bin/spacetime.exe"
          }
        }
      )
    }

    stage('Deploy') {
      parallel (
        spacetimedb: {
          withCredentials([usernamePassword(credentialsId: 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS', usernameVariable: 'USERNAME', passwordVariable: 'PASSWORD'),]) {
            withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
	      sh "ssh -o StrictHostKeyChecking=accept-new -i '${keyfile}' jenkins@${env.PARTNER_HOST} 'mkdir -p /home/jenkins/SpacetimeDB'"
              sh "scp -o StrictHostKeyChecking=accept-new -i '${keyfile}' .jenkins/deploy-spacetimedb.sh jenkins@${env.PARTNER_HOST}:/home/jenkins/deploy-spacetimedb.sh"
              sh "scp -o StrictHostKeyChecking=accept-new -i '${keyfile}' docker-compose-live.yml jenkins@${env.PARTNER_HOST}:/home/jenkins/SpacetimeDB/docker-compose-live.yml"
	      sh "ssh -o StrictHostKeyChecking=accept-new -i '${keyfile}' jenkins@${env.PARTNER_HOST} 'bash ./deploy-spacetimedb.sh '${USERNAME}' '${PASSWORD}' '${env.PARTNER_NAME}''"
            }
          }
	}, 
	cli_deploy: {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            // Upload linux, macos and windows executables
            sh "ssh -o StrictHostKeyChecking=accept-new -i '${keyfile}' jenkins@${env.PARTNER_HOST} 'rm -rf cli-bin; mkdir -p cli-bin'"
            sh "scp -o StrictHostKeyChecking=accept-new -i '${keyfile}' cli-bin/* jenkins@${env.PARTNER_HOST}:cli-bin/"

	    // Copy and deploy cli script
            sh "scp -o StrictHostKeyChecking=accept-new -i '${keyfile}' .jenkins/deploy-cli.sh jenkins@${env.PARTNER_HOST}:deploy-cli.sh"
            sh "ssh -o StrictHostKeyChecking=accept-new -i '${keyfile}' jenkins@${env.PARTNER_HOST} bash deploy-cli.sh"
          }
	}
      )
    }
  } catch (FlowInterruptedException interruptEx) {
    currentBuild.result = "ABORTED"
    throw interruptEx;
  } catch (err) {
    currentBuild.result = "FAILURE"
    throw err;
  } finally {
    // print discord message
    // discordSend description: "", link: env.BUILD_URL, result: currentBuild.currentResult, title: "", webhookURL: ""
  }
}
