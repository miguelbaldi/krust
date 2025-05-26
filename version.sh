# List git tags sorted based on semantic versioning
GIT_TAGS=$(git tag --sort=version:refname)
 
# Get last line of output which returns the
# last tag (most recent version)
GIT_TAG_LATEST=$(echo "$GIT_TAGS" | tail -n 1)
 
# If no tag found, default to v0.0.0
if [ -z "$GIT_TAG_LATEST" ]; then
  GIT_TAG_LATEST="$(date +%Y%m%d%H%M%S)-$(git describe --always)"
fi
 
# Strip prefix 'v' from the tag to easily increment
export GIT_TAG_LATEST=$(echo "$GIT_TAG_LATEST" | sed 's/^v//')

echo $GIT_TAG_LATEST
