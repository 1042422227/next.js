/** @type {import('next').NextConfig} */
module.exports = {
  deploymentId:
    process.env.IMMUTABLE_DEPLOYMENT_ID ?? process.env.CUSTOM_DEPLOYMENT_ID,
  experimental: {
    useSkewCookie: Boolean(process.env.COOKIE_SKEW),
    immutableDeploymentId: process.env.IMMUTABLE_DEPLOYMENT_ID
      ? `imm-${process.env.IMMUTABLE_DEPLOYMENT_ID}`
      : undefined,
  },
}
