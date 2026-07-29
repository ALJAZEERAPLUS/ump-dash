in home navigation

I want to add a review function accessible from + menu with the key 'r' for review.
it should open a prompt listing all the open PRs. use gh to fetch the list.
typing in this list should filter based on title + author. search ui should be conventional.
TAB should change the filtring. the filters are: ALL, Not reviewed, Mine, Not Mine
After selecting the PR is the branch is checked out already, a prompt should let the user know where it's already checked out.
otherwise an already filled text input is presented with the name of the branch - leting the user to customize the worktree name.
selecting a PR should checkout the same revision as what's on the remote. not sure if reset --hard or fetch then checkout or what but it should give us a sane worktree and it should be fool proof.
and then it should run yarn install.
I want to chain these parts while not breaking the existing architecture and have each step as the dependency of the next one.
ask clarifying questions when you are not sure instead of assuming
present the plan before executing and
