// This could also be a variable instead of a function, but some unit tests want to change the ID at
// runtime. Even though that would never happen in a real deployment.
export function getDeploymentId(): string | undefined {
  return process.env.NEXT_DEPLOYMENT_ID
}

export function getimmutableDeploymentId(): string | undefined {
  return process.env.NEXT_ASSET_DEPLOYMENT_ID || process.env.NEXT_DEPLOYMENT_ID
}

export function getimmutableDeploymentIdQuery(amperstand = false): string {
  let deploymentId = getimmutableDeploymentId()
  if (deploymentId) {
    return `${amperstand ? '&' : '?'}dpl=${deploymentId}`
  }
  return ''
}
