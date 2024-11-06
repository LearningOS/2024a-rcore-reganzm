use alloc::vec::Vec;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Resource {
    Mutex(usize),
    Semaphore(usize),
    Condvar(usize),
}
const MAX_THREADS: usize = 32;
const MAX_RESOURCES: usize = 32;

/// dead detector
pub struct DeadlockDetector {
    /// resources
    //pub resources: Vec<Option<Resource>>,
    pub resources: Vec<Option<Resource>>,
    /// available resource in current system
    pub available: Vec<usize>,
    /// allocated resource in current system
    pub allocationed: Vec<Vec<usize>>,
    /// needed resource in current system
    pub need: Vec<Vec<usize>>,
    /// taskids
    pub tasks: Vec<usize>,
}

impl DeadlockDetector {
    /// display
    pub fn display(&self, task_id: usize) {
        println!("---------------------------------------------");
        println!(
            r#"task_id:{} 
            resources:{:?}
            available:{:?}
            allocateioned:{:?}
            need:{:?}
            tasks:{:?}"#,
            task_id,
            self.resources,
            self.available,
            self.allocationed[task_id],
            self.need[task_id],
            self.tasks
        );
        println!("---------------------------------------------");
    }
    /// constructor
    pub fn new() -> Self {
        DeadlockDetector {
            resources: Vec::new(),
            available: alloc::vec![0;MAX_RESOURCES],
            allocationed: alloc::vec![alloc::vec![0;MAX_RESOURCES];MAX_THREADS],
            need: alloc::vec![alloc::vec![0;MAX_RESOURCES];MAX_THREADS],
            tasks: Vec::new(),
        }
    }
    /// add resource
    pub fn add_resource(&mut self, r: Resource, count: usize) {
        self.resources.push(Some(r));
        if let Some(res_id) = self.resource_index(r) {
            self.available[res_id] = count;
        } else {
            panic!("resource not found")
        }
        println!("add resource:{:?}, count:{}", r, count);
    }
    /// release resource
    pub fn release_resource(&mut self, task_id: usize, amount: usize, r: Resource) {
        println!(
            "release resource -> task_id:{} amount:{} r:{:?} ",
            task_id, amount, r
        );
        let Some(resource_id) = self.resource_index(r) else {
            panic!("resource {:?} not found", r);
        };

        self.available[resource_id] += amount;
        self.allocationed[task_id][resource_id] -= amount;
    }

    /// get resource index
    pub fn resource_index(&self, r: Resource) -> Option<usize> {
        self.resources.iter().position(|&res| res == Some(r))
    }

    /// ask for resource
    pub fn try_request(&mut self, task_id: usize, amount: usize, r: Resource) -> bool {
        let mut result = false;
        if !self.tasks.contains(&task_id) {
            self.tasks.push(task_id);
        }
        if let Some(res_id) = self.resource_index(r) {
            self.need[task_id][res_id] += amount;
            if self.deadlock_check() {
                result = true;
                println!("deadlock check is ok");
            } else {
                self.need[task_id][res_id] -= amount;
                self.tasks
                    .remove(self.tasks.iter().position(|&t| t == task_id).unwrap());
                println!("deadlock check is not ok");
            }
        } else {
            panic!("resouce not found!");
        }
        result
    }
    /// allocate resource
    pub fn request(&mut self, task_id: usize, amount: usize, r: Resource) {
        let Some(resource_id) = self.resource_index(r) else {
            panic!("resource not found");
        };

        assert!(
            amount <= self.available[resource_id],
            "request amount:{} available:{:?} resource_id:{}, task_id:{}, resource:{:?}",
            amount,
            self.available,
            resource_id,
            task_id,
            r
        );

        self.allocationed[task_id][resource_id] += amount;
        self.available[resource_id] -= amount;
        self.need[task_id][resource_id] -= amount;
    }

    /// dead lock check
    pub fn deadlock_check(&self) -> bool {
        let mut result = false;

        let mut work = self.available.clone();
        let task_count = self.tasks.len();
        let mut finish = alloc::vec![false;task_count];

        let mut count = 0;

        while count < task_count {
            let mut found = false;
            for task_id in 0..task_count {
                if finish[task_id] {
                    continue;
                }

                let mut is_safe = true;
                for res_id in 0..self.resources.len() {
                    if self.need[task_id][res_id] > work[res_id] {
                        is_safe = false;
                        break;
                    }
                }

                if is_safe {
                    for resid in 0..self.resources.len() {
                        work[resid] += self.allocationed[task_id][resid];
                    }
                    finish[task_id] = true;
                    count += 1;
                    found = true;
                }
            }
            if !found {
                println!("dead lock checked");
                return result;
            } else {
                result = true;
            }
        }
        println!("finish vec:{:?}", finish);

        result
    }
}
